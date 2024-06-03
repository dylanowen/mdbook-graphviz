use std::future::Future;
use std::mem;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use anyhow::{anyhow, Result};
use async_recursion::async_recursion;
use futures::future;
use mdbook::book::{Book, Chapter};
use mdbook::preprocess::PreprocessorContext;
use mdbook::utils::new_cmark_parser;
use mdbook::BookItem;
use pulldown_cmark::CodeBlockKind::Fenced;
use pulldown_cmark::{Event, Tag, TagEnd};
use pulldown_cmark_to_cmark::cmark;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::SvgRenderer;

#[derive(Default)]
pub struct SvgRendererSharedConfig {
    pub info_string: String,
    pub renderer: String,
    pub copy_js: Option<PathBuf>,
    pub copy_css: Option<PathBuf>,
    pub output_to_file: bool,
    pub link_to_file: bool,
}

pub trait SvgPreprocessor {
    type Renderer: SvgRenderer;

    fn name(&self) -> &str;

    fn default_info_string(&self) -> &str;

    fn build_renderer(
        &self,
        ctx: &PreprocessorContext,
        shared_config: SvgRendererSharedConfig,
    ) -> Result<Self::Renderer>;

    fn run(&self, ctx: &PreprocessorContext, mut book: Book) -> Result<Book> {
        let mut config = SvgRendererSharedConfig::default();
        config.renderer.clone_from(&ctx.renderer);

        if let Some(ctx_config) = ctx.config.get_preprocessor(self.name()) {
            config.info_string = if let Some(value) = ctx_config.get("info-string") {
                value
                    .as_str()
                    .ok_or_else(|| anyhow!("info-string option is required to be a string"))?
                    .to_string()
            } else {
                self.default_info_string().to_string()
            };

            if let Some(value) = ctx_config.get("copy-js") {
                config.copy_js = value
                    .as_bool()
                    .map(|v| if v { Some("js/svg.js".into()) } else { None })
                    .unwrap_or_else(|| {
                        value
                            .as_str()
                            .map(|v| Some(v.into()))
                            .expect("copy-js option is required to be a boolean or a string")
                    });
            }

            if let Some(value) = ctx_config.get("copy-css") {
                config.copy_css = value
                    .as_bool()
                    .map(|v| if v { Some("css/svg.css".into()) } else { None })
                    .unwrap_or_else(|| {
                        value
                            .as_str()
                            .map(|v| Some(v.into()))
                            .expect("copy-css option is required to be a boolean or a string")
                    });
            }

            if let Some(value) = ctx_config.get("output-to-file") {
                config.output_to_file = value
                    .as_bool()
                    .ok_or_else(|| anyhow!("output-to-file option is required to be a boolean"))?;
            }

            if let Some(value) = ctx_config.get("link-to-file") {
                config.link_to_file = value
                    .as_bool()
                    .ok_or_else(|| anyhow!("link-to-file option is required to be a boolean"))?;
            }
        }

        let renderer = self.build_renderer(ctx, config)?;
        tokio::runtime::Builder::new_multi_thread()
            .enable_io()
            .build()
            .unwrap()
            .block_on(async {
                const FILE_VERSION: &str =
                    concat!("/* mdBook-svg:", env!("CARGO_PKG_VERSION"), "*/");

                async fn browser_content_exists(location: &Path) -> Result<bool> {
                    if let Ok(mut file) = File::open(location).await {
                        let version_bytes = FILE_VERSION.as_bytes();
                        let mut buffer = vec![0; version_bytes.len()];
                        if file.read_exact(&mut buffer).await.is_ok() && buffer == version_bytes {
                            return Ok(true);
                        }
                    }

                    Ok(false)
                }

                async fn write_custom_browser_content(
                    location: &Path,
                    content: &str,
                ) -> Result<()> {
                    if let Ok(true) = browser_content_exists(location).await {
                        log::trace!("File already up to date {:?}", location);

                        if !cfg!(debug_assertions) {
                            return Ok(());
                        } else {
                            log::info!(
                                "File already up to date, updating for debug mode {:?}",
                                location
                            );
                        }
                    }

                    log::info!("Creating/Updating to {:?}", location);

                    let mut file = File::create(location).await?;
                    let full_content = format!("{}{}", FILE_VERSION, content);
                    file.write_all(full_content.as_bytes()).await?;

                    Ok(())
                }

                if let Some(js_output_file) = renderer.copy_js() {
                    const SVG_JS: &str = include_str!("../dist/svg.js");

                    write_custom_browser_content(&ctx.root.join(js_output_file), SVG_JS).await?;
                }

                if let Some(css_file) = &renderer.copy_css() {
                    const SVG_CSS: &str = include_str!("../dist/svg.css");

                    write_custom_browser_content(&ctx.root.join(css_file), SVG_CSS).await?;
                }

                let book_src_dir = ctx.root.join(&ctx.config.book.src);

                self.process_sub_items(&renderer, &mut book.sections, &book_src_dir)
                    .await
            })?;

        Ok(book)
    }

    #[async_recursion(?Send)]
    async fn process_sub_items(
        &'async_recursion self,
        renderer: &Self::Renderer,
        items: &mut Vec<BookItem>,
        book_src_dir: &Path,
    ) -> Result<()> {
        let mut item_futures = Vec::with_capacity(items.len());
        for item in mem::take(items) {
            item_futures.push(async {
                match item {
                    BookItem::Chapter(chapter) => self
                        .process_chapter(renderer, chapter, book_src_dir)
                        .await
                        .map(BookItem::Chapter),
                    item => {
                        // pass through all non-chapters
                        Ok(item)
                    }
                }
            });
        }

        *items = future::join_all(item_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;

        Ok(())
    }

    #[async_recursion(?Send)]
    async fn process_chapter(
        &self,
        renderer: &Self::Renderer,
        mut chapter: Chapter,
        book_src_dir: &Path,
    ) -> Result<Chapter> {
        // make sure to process our chapter sub-items
        self.process_sub_items(renderer, &mut chapter.sub_items, book_src_dir)
            .await?;

        if chapter.path.is_none() {
            return Ok(chapter);
        }

        let mut buf = String::with_capacity(chapter.content.len());
        let mut block_builder: ParsingState = ParsingState::PassingEvents(vec![]);
        let mut image_index = 0;

        let events = new_cmark_parser(&chapter.content, false);
        #[allow(clippy::type_complexity)]
        let mut event_futures: Vec<Pin<Box<dyn Future<Output = Result<Vec<Event>>>>>> = vec![];

        for (e, byte_offset) in events.into_offset_iter() {
            match mem::take(&mut block_builder) {
                ParsingState::BuildingBlock(mut builder) => {
                    match e {
                        Event::Text(ref text) => {
                            builder.append_source_code(text.to_string());
                            block_builder = ParsingState::BuildingBlock(builder);
                        }
                        Event::End(TagEnd::CodeBlock) => {
                            // start rendering our diagram
                            let block = builder.build(image_index);
                            image_index += 1;

                            event_futures.push(Box::pin(renderer.render(block)));
                        }
                        _ => {
                            block_builder = ParsingState::BuildingBlock(builder);
                        }
                    }
                }
                ParsingState::PassingEvents(mut events) => {
                    if let Event::Start(Tag::CodeBlock(Fenced(info_string))) = &e {
                        let prefix_len = renderer.info_string().len();
                        // The following split is safe because the characters have
                        // to be byte equal to be a match, therefore we are
                        // guaranteed to split at a character boundary.
                        let (prefix, graph_name) =
                            info_string.split_at(std::cmp::min(info_string.len(), prefix_len));
                        if prefix == renderer.info_string() {
                            // better line numbers with diff from original file? https://blog.jcoglan.com/2017/02/15/the-myers-diff-algorithm-part-2/
                            let line_number = chapter
                                .content
                                .bytes()
                                .take(byte_offset.start)
                                .filter(|&b| b == b'\n')
                                .count()
                                + 2; // add 1 for 0-indexing and 1 for the code block start

                            // check if we can have a name at the end of our info string
                            block_builder = ParsingState::BuildingBlock(SvgBlockBuilder::new(
                                chapter.name.clone().trim().to_string(),
                                book_src_dir.to_path_buf(),
                                // assume we've already filtered out all the draft chapters
                                chapter.path.clone().unwrap(),
                                self.name().to_string(),
                                Some(graph_name.trim().to_string()).filter(|s| !s.is_empty()),
                                line_number,
                            ));

                            // pass through all events before this start block
                            event_futures.push(Box::pin(async { Ok(events) }));

                            continue;
                        }
                    }

                    events.push(e);

                    // pass through all events that don't impact our Graphviz block
                    block_builder = ParsingState::PassingEvents(events);
                }
            }
        }

        // finish out our remaining block builder
        match block_builder {
            ParsingState::BuildingBlock(builder) => {
                // just treat remaining blocks as if we ended it
                let block = builder.build(image_index);

                log::warn!(
                    "{}: Found unclosed {} block",
                    block.location_string(None, None),
                    self.name()
                );

                event_futures.push(Box::pin(renderer.render(block)));
            }
            ParsingState::PassingEvents(events) => {
                if !events.is_empty() {
                    event_futures.push(Box::pin(async { Ok(events) }));
                }
            }
        }

        let events = future::join_all(event_futures)
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten();

        cmark(events, &mut buf)?;

        chapter.content = buf;

        Ok(chapter)
    }
}

pub struct SvgBlock {
    source_code: String,
    /// the line where our code block starts. Ex: ```dot process
    source_code_initial_line: usize,
    book_path: PathBuf,
    chapter_relative_path: PathBuf,
    preprocessor_name: String,
    chapter_name: String,
    graph_name: Option<String>,
    index: usize,
}

impl SvgBlock {
    pub fn source_code(&self) -> &str {
        &self.source_code
    }

    /// Unique across all graphs in the chapter for all svg preprocessors
    pub fn uid_for_chapter(&self) -> String {
        format!("{}_{}", normalize_id(&self.preprocessor_name), self.index,)
    }

    /// Unique (and "pretty") across all graphs in the book for all svg preprocessors
    pub fn svg_file_name(&self, relative_id: Option<&str>) -> String {
        format!(
            "{}{}_{}_{}{}.generated.svg",
            normalize_id(&self.chapter_name),
            self.graph_name
                .as_ref()
                .map(|s| format!("_{}", normalize_id(s)))
                .unwrap_or_default(),
            normalize_id(&self.preprocessor_name),
            self.index,
            relative_id
                .map(|s| format!("_{}", normalize_id(s)))
                .unwrap_or_default(),
        )
    }

    pub fn chapter_path(&self) -> PathBuf {
        let mut chapter_dir = self.book_path.join(&self.chapter_relative_path).clone();
        chapter_dir.pop();
        chapter_dir
    }

    pub fn graph_name(&self) -> Option<String> {
        self.graph_name.clone()
    }

    pub fn location_string<S, E>(
        &self,
        inline_line_number_start: S,
        inline_line_number_end: E,
    ) -> String
    where
        S: Into<Option<usize>>,
        E: Into<Option<usize>>,
    {
        let start_line_number =
            self.source_code_initial_line + inline_line_number_start.into().unwrap_or_default();
        let end_line_number = inline_line_number_end
            .into()
            .map(|o| self.source_code_initial_line + o);

        format!(
            "{}({}{})",
            self.chapter_relative_path.to_string_lossy(),
            start_line_number,
            end_line_number.map(|e| format!(":{e}")).unwrap_or_default()
        )
    }
}

fn normalize_id(content: &str) -> String {
    content
        .chars()
        .filter_map(|ch| {
            if ch.is_alphanumeric() {
                Some(ch.to_ascii_lowercase())
            } else if ch.is_whitespace() || ch == '_' || ch == '-' {
                Some('_')
            } else {
                None
            }
        })
        .collect::<String>()
}

enum ParsingState<'a> {
    BuildingBlock(SvgBlockBuilder),
    PassingEvents(Vec<Event<'a>>),
}

impl<'a> Default for ParsingState<'a> {
    fn default() -> Self {
        ParsingState::PassingEvents(vec![])
    }
}

pub(crate) struct SvgBlockBuilder {
    source_code: String,
    /// the line where our code block starts. Ex: ```dot process
    source_code_initial_line: usize,
    book_path: PathBuf,
    chapter_relative_path: PathBuf,
    preprocessor_name: String,
    chapter_name: String,
    graph_name: Option<String>,
}

impl SvgBlockBuilder {
    pub(crate) fn new(
        chapter_name: String,
        book_path: PathBuf,
        chapter_relative_path: PathBuf,
        preprocessor_name: String,
        graph_name: Option<String>,
        source_code_initial_line: usize,
    ) -> SvgBlockBuilder {
        SvgBlockBuilder {
            source_code: String::new(),
            source_code_initial_line,
            book_path,
            chapter_relative_path,
            preprocessor_name,
            chapter_name,
            graph_name,
        }
    }

    pub(crate) fn append_source_code<S: Into<String>>(&mut self, code: S) {
        self.source_code.push_str(&code.into());
    }

    pub(crate) fn build(self, index: usize) -> SvgBlock {
        SvgBlock {
            source_code: self.source_code,
            source_code_initial_line: self.source_code_initial_line,
            book_path: self.book_path,
            chapter_relative_path: self.chapter_relative_path,
            preprocessor_name: self.preprocessor_name,
            chapter_name: self.chapter_name,
            graph_name: self.graph_name,
            index,
        }
    }
}

#[cfg(test)]
mod test {
    // use async_trait::async_trait;

    use crate::renderer::test::TestRenderer;
    use crate::renderer::{D2_CONTAINER_CLASS, TAB_CONTENT_CLASS};

    use super::*;

    static CHAPTER_NAME: &str = "Test Chapter";
    static NORMALIZED_CHAPTER_NAME: &str = "test_chapter";

    // use std::time::{Duration, Instant};
    //
    // static CHAPTER_NAME: &str = "Test Chapter";
    // static NORMALIZED_CHAPTER_NAME: &str = "test_chapter";
    //
    // struct NoopRenderer;
    //
    // #[async_trait]
    // impl GraphvizRendererOld for NoopRenderer {
    //     async fn render_graphviz<'a>(
    //         block: GraphvizBlock,
    //         _config: &GraphvizConfig,
    //     ) -> Result<Vec<Event<'a>>> {
    //         let file_name = block.svg_file_name();
    //         let output_path = block.svg_output_path();
    //         let GraphvizBlock {
    //             graph_name, index, ..
    //         } = block;
    //
    //         Ok(vec![Event::Text(
    //             format!("{file_name}|{output_path:?}|{graph_name}|{index}").into(),
    //         )])
    //     }
    // }

    #[tokio::test]
    async fn only_preprocess_flagged_blocks() {
        let expected = r#"# Chapter

````svg
digraph Test {
    a -> b
}
````"#;
        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                info_string: "svg process".to_string(),
                ..Default::default()
            },
        };
        let chapter = TestPreprocessor
            .process_chapter(&renderer, new_chapter(expected), Path::new(""))
            .await
            .unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn preprocess_flagged_blocks_with_custom_flag() {
        let chapter = new_chapter(
            r#"# Chapter
```custom
digraph Test {
    a -> b
}
```
"#,
        );
        let expected = format!(
            r#"# Chapter



<div class="{D2_CONTAINER_CLASS}"><div><div id="{TAB_CONTENT_CLASS}-test_0" class="{TAB_CONTENT_CLASS} mdbook-graphviz-output">result</div></div></div>

"#
        );

        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                info_string: "custom".to_string(),
                ..Default::default()
            },
        };
        let chapter = TestPreprocessor
            .process_chapter(&renderer, chapter, Path::new(""))
            .await
            .unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn do_not_preprocess_flagged_blocks_without_custom_flag() {
        let expected = r#"# Chapter

````dot
digraph Test {
    a -> b
}
````"#;

        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                info_string: "svg".to_string(),
                ..Default::default()
            },
        };
        let chapter = TestPreprocessor
            .process_chapter(&renderer, new_chapter(expected), Path::new(""))
            .await
            .unwrap();

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn no_name() {
        let chapter = new_chapter(
            r#"# Chapter
```dot process
digraph Test {
    a -> b
}
```
"#,
        );

        let expected = format!(
            r#"# Chapter

{NORMALIZED_CHAPTER_NAME}_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_0.generated.svg"||0"#
        );

        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                info_string: "svg".to_string(),
                ..Default::default()
            },
        };
        let chapter = TestPreprocessor
            .process_chapter(&renderer, chapter, Path::new(""))
            .await
            .unwrap();

        println!("{}", expected);

        assert_eq!(chapter.content, expected);
    }

    #[tokio::test]
    async fn named_blocks() {
        let chapter = new_chapter(
            r#"# Chapter
```svg Graph Name
digraph Test {
    a -> b
}
```
"#,
        );

        let expected = format!(
            r#"# Chapter

{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
        );

        let renderer = TestRenderer {
            config: SvgRendererSharedConfig {
                info_string: "svg".to_string(),
                ..Default::default()
            },
        };
        let chapter = TestPreprocessor
            .process_chapter(&renderer, chapter, Path::new(""))
            .await
            .unwrap();

        assert_eq!(chapter.content, expected);
    }

    struct TestPreprocessor;

    impl SvgPreprocessor for TestPreprocessor {
        type Renderer = TestRenderer;

        fn name(&self) -> &str {
            "test"
        }

        fn default_info_string(&self) -> &str {
            "test"
        }

        fn build_renderer(
            &self,
            _ctx: &PreprocessorContext,
            shared_config: SvgRendererSharedConfig,
        ) -> Result<Self::Renderer> {
            Ok(TestRenderer {
                config: shared_config,
            })
        }
    }

    fn new_chapter<S: ToString>(content: S) -> Chapter {
        Chapter::new(
            CHAPTER_NAME,
            content.to_string(),
            PathBuf::from("./book/chapter.md"),
            vec![],
        )
    }
}
