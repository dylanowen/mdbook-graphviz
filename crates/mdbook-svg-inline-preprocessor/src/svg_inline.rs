use lazy_static::lazy_static;
use regex::Regex;
use std::collections::HashMap;
use xml::name::OwnedName;
use xml::reader::XmlEvent;
use xml::{EmitterConfig, ParserConfig};

lazy_static! {
    static ref NEWLINES_RE: Regex = Regex::new(r"\n\n+").unwrap();
}

pub(crate) fn format_for_inline(output: &str, id_prefix: &str) -> String {
    // try the advanced mapping first and fallback on the basic stuff
    match format_for_inline_advanced(output, id_prefix) {
        Ok(output) => output,
        Err(e) => {
            log::warn!("Error parsing SVG: {}", e);
            format_for_inline_simple(output)
        }
    }
}

/// SVGs can have ids which must be unique across the html document. This function attempts to prefix
/// all of them with an id_prefix to ensure uniqueness.a
fn format_for_inline_advanced(output: &str, id_prefix: &str) -> Result<String, String> {
    let id_name = OwnedName {
        local_name: "id".to_string(),
        namespace: None,
        prefix: None,
    };

    fn replace_mapped_ids(value: &str, mapped_ids: &HashMap<String, String>) -> String {
        let mut value = value.to_string();
        for (old_id, new_id) in mapped_ids.iter() {
            value = value.replace(old_id, new_id);
        }
        value
    }
    let mut mapped_ids = HashMap::new();
    let mut reader = ParserConfig::new()
        .trim_whitespace(true)
        .create_reader(output.as_bytes());
    let mut events = Vec::new();

    // pull out all the ids and prefix them with our unique id
    loop {
        match reader
            .next()
            .map_err(|e| format!("Error parsing SVG: {e}"))?
        {
            XmlEvent::StartElement {
                name,
                mut attributes,
                namespace,
            } => {
                for attribute in attributes.iter_mut() {
                    if attribute.name == id_name {
                        let id = &attribute.value.clone();
                        let new_id = format!("{id_prefix}-{id}");
                        attribute.value = new_id.clone();
                        mapped_ids.insert(format!("#{id}"), format!("#{new_id}"));
                    }
                }

                events.push(XmlEvent::StartElement {
                    name,
                    attributes,
                    namespace,
                });
            }
            event @ XmlEvent::EndDocument => {
                events.push(event);
                break;
            }
            event => events.push(event),
        }
    }

    let mut buffer = Vec::new();
    let mut writer = EmitterConfig::new()
        .line_separator("")
        .write_document_declaration(false)
        .keep_element_names_stack(false)
        .create_writer(&mut buffer);

    // replace all references of #<id> with our new remapped id
    for mut event in events {
        match event {
            XmlEvent::StartElement {
                ref mut attributes, ..
            } => {
                for attribute in attributes.iter_mut() {
                    attribute.value = replace_mapped_ids(&attribute.value, &mapped_ids);
                }
            }
            XmlEvent::CData(ref mut value)
            | XmlEvent::Comment(ref mut value)
            | XmlEvent::Characters(ref mut value) => {
                // remove explicit newlines as they won't be preserved and break commonmark parsing
                *value = NEWLINES_RE
                    .replace_all(&replace_mapped_ids(&value, &mapped_ids), "\n")
                    .to_string();
            }
            _ => (),
        }

        match event {
            // drop our start document event
            XmlEvent::StartDocument { .. } => {}
            event => {
                if let Some(writer_event) = event.as_writer_event() {
                    writer
                        .write(writer_event)
                        .map_err(|e| format!("Error writing SVG: {e}"))?;
                }
            }
        }
    }

    String::from_utf8(buffer).map_err(|e| format!("Error converting SVG to string: {e}",))
}

fn format_for_inline_simple(output: &str) -> String {
    lazy_static! {
        static ref DOCTYPE_RE: Regex = Regex::new(r"<!DOCTYPE [^>]+>").unwrap();
        static ref XML_TAG_RE: Regex = Regex::new(r"<\?xml [^>]+\?>").unwrap();
        static ref NEW_LINE_TAGS_RE: Regex = Regex::new(r">\s+<").unwrap();
    }

    // yes yes: https://stackoverflow.com/a/1732454 ZA̡͊͠͝LGΌ and such
    let output = DOCTYPE_RE.replace(&output, "");
    let output = XML_TAG_RE.replace(&output, "");
    // remove newlines between our tags to help commonmark determine the full set of HTML
    let output = NEW_LINE_TAGS_RE.replace_all(&output, "><");
    // remove explicit newlines as they won't be preserved and break commonmark parsing
    let output = NEWLINES_RE.replace_all(&output, "\n");
    let output = output.trim();

    output.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdbook::utils::new_cmark_parser;
    use pulldown_cmark::{Event, Parser};
    use pulldown_cmark_to_cmark::cmark;
    use std::borrow::Borrow;

    #[test]
    fn test_inline_advanced() {
        let events = [Event::Html(
            format!(
                "<div>{}</div>",
                format_for_inline_advanced(include_str!("../tests/d2.svg"), "test").unwrap()
            )
            .into(),
        )];
        let mut serialized_string = String::new();
        cmark(events.into_iter(), &mut serialized_string).unwrap();

        let mut parsed_events = new_cmark_parser(&serialized_string, false).collect::<Vec<_>>();

        assert_eq!(parsed_events.len(), 1);
        assert!(matches!(parsed_events[0], Event::Html(_)));
    }
}
