use anyhow::anyhow;
use mdbook::errors::Result;
use mdbook::preprocess::PreprocessorContext;

use mdbook_svg_inline_preprocessor::{run_preprocessor, SvgPreprocessor, SvgRendererSharedConfig};

use crate::renderer::GraphvizRenderer;

mod renderer;

pub static PREPROCESSOR_NAME: &str = "graphviz";
pub static DEFAULT_INFO_STRING_PREFIX: &str = "dot process";

fn main() {
    run_preprocessor(&GraphvizPreprocessor);
}

pub struct GraphvizPreprocessor;

impl SvgPreprocessor for GraphvizPreprocessor {
    type Renderer = GraphvizRenderer;

    fn name(&self) -> &str {
        PREPROCESSOR_NAME
    }

    fn default_info_string(&self) -> &str {
        DEFAULT_INFO_STRING_PREFIX
    }

    fn build_renderer(
        &self,
        ctx: &PreprocessorContext,
        config: SvgRendererSharedConfig,
    ) -> Result<Self::Renderer> {
        let mut renderer = GraphvizRenderer::new(config);

        if let Some(ctx_config) = ctx.config.get_preprocessor(self.name()) {
            if let Some(value) = ctx_config.get("arguments") {
                renderer.arguments = value
                    .as_array()
                    .ok_or_else(|| anyhow!("arguments option is required to be an array"))?
                    .iter()
                    .map(|v| {
                        v.as_str().map(str::to_string).ok_or_else(|| {
                            anyhow!("arguments option is required to contain strings")
                        })
                    })
                    .collect::<Result<Vec<_>>>()?
            }
        }

        Ok(renderer)
    }
}

// #[cfg(test)]
// mod test {
//     use async_trait::async_trait;
//
//     use super::*;
//
//     use std::time::{Duration, Instant};
//
//     static CHAPTER_NAME: &str = "Test Chapter";
//     static NORMALIZED_CHAPTER_NAME: &str = "test_chapter";
//
//     struct NoopRenderer;
//
//     #[async_trait]
//     impl GraphvizRendererOld for NoopRenderer {
//         async fn render_graphviz<'a>(
//             block: GraphvizBlock,
//             _config: &GraphvizConfig,
//         ) -> Result<Vec<Event<'a>>> {
//             let file_name = block.svg_file_name();
//             let output_path = block.svg_output_path();
//             let GraphvizBlock {
//                 graph_name, index, ..
//             } = block;
//
//             Ok(vec![Event::Text(
//                 format!("{file_name}|{output_path:?}|{graph_name}|{index}").into(),
//             )])
//         }
//     }
//
//     #[tokio::test]
//     async fn only_preprocess_flagged_blocks() {
//         let expected = r#"# Chapter
//
// ````dot
// digraph Test {
//     a -> b
// }
// ````"#;
//         let chapter = process_chapter(new_chapter(expected)).await.unwrap();
//
//         assert_eq!(chapter.content, expected);
//     }
//
//     #[tokio::test]
//     async fn preprocess_flagged_blocks_with_custom_flag() {
//         let chapter = new_chapter(
//             r#"# Chapter
// ```graphviz
// digraph Test {
//     a -> b
// }
// ```
// "#,
//         );
//         let expected = format!(
//             r#"# Chapter
//
// {NORMALIZED_CHAPTER_NAME}_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_0.generated.svg"||0"#
//         );
//
//         let config = GraphvizConfig {
//             info_string: "graphviz".to_string(),
//             ..GraphvizConfig::default()
//         };
//         let chapter = process_chapter_with_config(chapter, config).await.unwrap();
//
//         assert_eq!(chapter.content, expected);
//     }
//
//     #[tokio::test]
//     async fn do_not_preprocess_flagged_blocks_without_custom_flag() {
//         let expected = r#"# Chapter
//
// ````dot
// digraph Test {
//     a -> b
// }
// ````"#;
//
//         let config = GraphvizConfig {
//             info_string: "graphviz".to_string(),
//             ..GraphvizConfig::default()
//         };
//         let chapter = process_chapter_with_config(new_chapter(expected), config)
//             .await
//             .unwrap();
//
//         assert_eq!(chapter.content, expected);
//     }
//
//     #[tokio::test]
//     async fn no_name() {
//         let chapter = new_chapter(
//             r#"# Chapter
// ```dot process
// digraph Test {
//     a -> b
// }
// ```
// "#,
//         );
//
//         let expected = format!(
//             r#"# Chapter
//
// {NORMALIZED_CHAPTER_NAME}_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_0.generated.svg"||0"#
//         );
//
//         let chapter = process_chapter(chapter).await.unwrap();
//
//         assert_eq!(chapter.content, expected);
//     }
//
//     #[tokio::test]
//     async fn named_blocks() {
//         let chapter = new_chapter(
//             r#"# Chapter
// ```dot process Graph Name
// digraph Test {
//     a -> b
// }
// ```
// "#,
//         );
//
//         let expected = format!(
//             r#"# Chapter
//
// {NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
//         );
//
//         let chapter = process_chapter(chapter).await.unwrap();
//
//         assert_eq!(chapter.content, expected);
//     }
//
//     #[tokio::test]
//     async fn preserve_escaping() {
//         let chapter = new_chapter(
//             r"# Chapter
//
// *asteriks*
// /*asteriks/*
// ( \int x dx = \frac{x^2}{2} + C)
//
// ```dot process Graph Name
// digraph Test {
//     a -> b
// }
// ```
// ",
//         );
//
//         let expected = format!(
//             r#"# Chapter
//
// *asteriks*
// /*asteriks/*
// ( \int x dx = \frac{{x^2}}{{2}} + C)
//
// {NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
//         );
//
//         let chapter = process_chapter(chapter).await.unwrap();
//
//         assert_eq!(chapter.content, expected);
//     }
//
//     #[tokio::test]
//     async fn preserve_tables() {
//         let chapter = new_chapter(
//             r#"# Chapter
//
// |Tables|Are|Cool|
// |------|:-:|---:|
// |col 1 is|left-aligned|$1600|
// |col 2 is|centered|$12|
// |col 3 is|right-aligned|$1|
//
// ```dot process Graph Name
// digraph Test {
//     a -> b
// }
// ```
// "#,
//         );
//
//         let expected = format!(
//             r#"# Chapter
//
// |Tables|Are|Cool|
// |------|:-:|---:|
// |col 1 is|left-aligned|$1600|
// |col 2 is|centered|$12|
// |col 3 is|right-aligned|$1|
//
// {NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
//         );
//
//         let chapter = process_chapter(chapter).await.unwrap();
//
//         assert_eq!(chapter.content, expected);
//     }
//
//     const SLEEP_DURATION: Duration = Duration::from_millis(100);
//     struct SleepyRenderer;
//     #[async_trait]
//     impl GraphvizRendererOld for SleepyRenderer {
//         async fn render_graphviz<'a>(
//             _block: GraphvizBlock,
//             _config: &GraphvizConfig,
//         ) -> Result<Vec<Event<'a>>> {
//             tokio::time::sleep(SLEEP_DURATION).await;
//             Ok(vec![Event::Text("".into())])
//         }
//     }
//
//     /// Test that we are actually running Graphviz concurrently
//     #[tokio::test]
//     async fn concurrent_execution() {
//         const TOTAL_CHAPTERS: usize = 10;
//         let mut chapters = Vec::with_capacity(TOTAL_CHAPTERS);
//         for _ in 0..TOTAL_CHAPTERS {
//             chapters.push(BookItem::Chapter(new_chapter(
//                 r#"# Chapter
// ```dot process Graph Name
// digraph Test {
//     a -> b
// }
// ```
// "#,
//             )));
//         }
//
//         let start = Instant::now();
//         Graphviz::<SleepyRenderer>::new(PathBuf::from("/"), GraphvizConfig::default())
//             .process_sub_items(&mut chapters)
//             .await
//             .unwrap();
//         let duration = start.elapsed();
//
//         for item in chapters {
//             if let BookItem::Chapter(chapter) = item {
//                 // make sure we used the correct renderer
//                 assert_eq!(chapter.content, "# Chapter\n\n");
//             } else {
//                 panic!("We should only have chapters here");
//             }
//         }
//
//         assert!(
//             duration < SLEEP_DURATION * 2,
//             "{duration:?} should be less than 2 * {SLEEP_DURATION:?} since we expect some variation when running"
//         );
//     }
//
//     /// Test that we correctly process Chapter sub-items
//     #[tokio::test]
//     async fn chapter_sub_items() {
//         let content = r#"# Chapter
//
// ```dot process Graph Name
// digraph Test {
//     a -> b
// }
// ```
// "#;
//         let mut chapter = new_chapter(content);
//         chapter
//             .sub_items
//             .push(BookItem::Chapter(new_chapter(content)));
//
//         let expected = format!(
//             r#"# Chapter
//
// {NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
//         );
//
//         let mut chapter = process_chapter(chapter).await.unwrap();
//
//         assert_eq!(chapter.content, expected);
//         if let BookItem::Chapter(child_chapter) = chapter.sub_items.remove(0) {
//             assert_eq!(child_chapter.content, expected);
//         }
//     }
//
//     #[tokio::test]
//     async fn skip_draft_chapters() {
//         let draft_chapter = Chapter::new_draft(CHAPTER_NAME, vec![]);
//         let mut book_items = vec![
//             BookItem::Chapter(draft_chapter.clone()),
//             BookItem::Chapter(new_chapter(
//                 r#"# Chapter
// ```dot process Graph Name
// digraph Test {
//     a -> b
// }
// ```
// "#,
//             )),
//         ];
//
//         Graphviz::<NoopRenderer>::new(PathBuf::from("/"), GraphvizConfig::default())
//             .process_sub_items(&mut book_items)
//             .await
//             .unwrap();
//
//         assert_eq!(
//             book_items,
//             vec![
//                 BookItem::Chapter(draft_chapter),
//                 BookItem::Chapter(new_chapter(format!(
//                     r#"# Chapter
//
// {NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg|"/./book/{NORMALIZED_CHAPTER_NAME}_graph_name_0.generated.svg"|Graph Name|0"#
//                 )))
//             ]
//         )
//     }
//
//     async fn process_chapter(chapter: Chapter) -> Result<Chapter> {
//         process_chapter_with_config(chapter, GraphvizConfig::default()).await
//     }
//
//     async fn process_chapter_with_config(
//         chapter: Chapter,
//         config: GraphvizConfig,
//     ) -> Result<Chapter> {
//         Graphviz::<NoopRenderer>::new(PathBuf::from("/"), config)
//             .process_chapter(chapter)
//             .await
//     }
//
//     fn new_chapter<S: ToString>(content: S) -> Chapter {
//         Chapter::new(
//             CHAPTER_NAME,
//             content.to_string(),
//             PathBuf::from("./book/chapter.md"),
//             vec![],
//         )
//     }
// }
