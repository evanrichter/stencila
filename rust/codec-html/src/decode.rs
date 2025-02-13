use std::cmp::max;

use kuchiki::{traits::*, ElementData, NodeRef};
use markup5ever::{local_name, LocalName};

use codec::{common::eyre::Result, CodecTrait};
use codec_txt::TxtCodec;
use node_transform::Transform;
use stencila_schema::*;

/// Decode a HTML document to a `Node`
///
/// Intended for decoding an entire document into an `Article`.
pub fn decode(html: &str, text_decoder: TextDecoder) -> Result<Node> {
    let content = decode_fragment(html, text_decoder);

    let article = Article {
        content: Some(content),
        ..Default::default()
    };

    Ok(Node::Article(article))
}

/// Decode a HTML fragment to a vector of `BlockContent`
///
/// Intended for decoding a fragment of HTML (e.g. some HTML in a
/// Markdown document) and inserting it into a larger document.
///
/// If any block content is present in the fragment then that will be returned.
/// Otherwise, if the fragment only consists of inline content a vector with
/// a single paragraph containing that content will be returned.
pub fn decode_fragment(html: &str, text_decoder: TextDecoder) -> Vec<BlockContent> {
    if html.is_empty() {
        return vec![];
    }

    let context = DecodeContext { text_decoder };
    let document = kuchiki::parse_html().one(html);

    let content = decode_blocks(&document, &context);
    if !content.is_empty() {
        return content;
    }

    let content = decode_inlines(&document, &context);
    if !content.is_empty() {
        return vec![BlockContent::Paragraph(Paragraph {
            content,
            ..Default::default()
        })];
    }

    vec![]
}

/// Function for optionally decoding text withing HTML elements
type TextDecoder = Option<Box<dyn Fn(&str) -> Vec<BlockContent>>>;

/// Decoding context
struct DecodeContext {
    text_decoder: TextDecoder,
}

/// Decode the children of a HTML node into a vector of `BlockContent`
fn decode_blocks(node: &NodeRef, context: &DecodeContext) -> Vec<BlockContent> {
    node.children()
        .flat_map(|child| decode_block(&child, context))
        .collect()
}

/// Decode a HTML node into a zero or more `BlockContent` nodes
///
/// Will ignore elements that are dealt with by `decode_inline`
fn decode_block(node: &NodeRef, context: &DecodeContext) -> Vec<BlockContent> {
    if let Some(_document) = node.as_document() {
        // Recurse into document
        decode_blocks(node, context)
    } else if let Some(element) = node.as_element() {
        let tag = element.name.local.clone();
        // Decode a HTML element
        //
        // Custom elements must be dealt with outside of the following match.
        if tag == *"stencila-code-block" || tag == *"stencila-code-chunk" {
            let programming_language = element
                .attributes
                .borrow()
                .get(LocalName::from("programming-language"))
                .map(|value| value.to_string());

            let text = if let Ok(text) = node.select_first("[slot=text]") {
                collect_text(text.as_node())
            } else {
                "".to_string()
            };

            return if tag == *"stencila-code-block" {
                vec![BlockContent::CodeBlock(CodeBlock {
                    programming_language: programming_language.map(Box::new),
                    text,
                    ..Default::default()
                })]
            } else {
                vec![BlockContent::CodeChunk(CodeChunk {
                    programming_language: programming_language.unwrap_or_default(),
                    text,
                    ..Default::default()
                })]
            };
        }
        if tag == *"stencila-include" || tag == *"stencila-call" {
            let source = element
                .attributes
                .borrow()
                .get(LocalName::from("source"))
                .unwrap_or_default()
                .to_string();

            let media_type = element
                .attributes
                .borrow()
                .get(LocalName::from("media-type"))
                .map(|value| Box::new(value.to_string()));

            let select = element
                .attributes
                .borrow()
                .get(LocalName::from("select"))
                .map(|value| Box::new(value.to_string()));

            let node = if tag == *"stencila-include" {
                BlockContent::Include(Include {
                    source,
                    media_type,
                    select,
                    ..Default::default()
                })
            } else {
                BlockContent::Call(Call {
                    source,
                    media_type,
                    select,
                    ..Default::default()
                })
            };

            return vec![node];
        }
        // The following are ordered alphabetically by the output node type
        // with placeholder comments for types not implemented yet.
        match tag {
            // TODO: Claim
            local_name!("pre") => {
                let programming_language = if let Ok(code) = node.select_first("code") {
                    code.attributes
                        .borrow()
                        .get(local_name!("class"))
                        .map(|value| Box::new(value.replace("language-", "")))
                } else {
                    None
                };
                let text = collect_text(node);

                vec![BlockContent::CodeBlock(CodeBlock {
                    text,
                    programming_language,
                    ..Default::default()
                })]
            }
            // TODO: Collection
            local_name!("figure") => {
                // Currently code chunks are wrapped in a figure, so check for them...
                if let Ok(code_chunk) = node.select_first("stencila-code-chunk") {
                    decode_block(code_chunk.as_node(), context)
                } else {
                    // TODO: handle plain figures
                    vec![]
                }
            }
            local_name!("h1")
            | local_name!("h2")
            | local_name!("h3")
            | local_name!("h4")
            | local_name!("h5")
            | local_name!("h6") => {
                let id = get_id(element);
                let depth = element.name.local.strip_prefix('h').map(|depth| {
                    // See the `Heading.to_html` for the rationale for
                    // subtracting one from the depth
                    let depth = depth.parse().unwrap_or(1);
                    max(1, depth - 1)
                });
                let content = decode_inlines(node, context);
                vec![BlockContent::Heading(Heading {
                    content,
                    depth,
                    id,
                    ..Default::default()
                })]
            }
            local_name!("ul") | local_name!("ol") => {
                let order = Some(match element.name.local {
                    local_name!("ol") => ListOrder::Ascending,
                    _ => ListOrder::Unordered,
                });
                let items = decode_list_items(node, context);

                vec![BlockContent::List(List {
                    items,
                    order,
                    ..Default::default()
                })]
            }
            // TODO: MathBlock
            local_name!("p") => {
                vec![BlockContent::Paragraph(Paragraph {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            local_name!("blockquote") => {
                vec![BlockContent::QuoteBlock(QuoteBlock {
                    content: decode_blocks(node, context),
                    ..Default::default()
                })]
            }
            local_name!("table") => {
                let mut rows = vec![];
                for child in node.children() {
                    if let Some(element) = child.as_element() {
                        match element.name.local {
                            local_name!("thead") => rows.append(&mut decode_table_rows(
                                &child,
                                &Some(TableRowRowType::Header),
                                context,
                            )),
                            local_name!("tbody") => {
                                rows.append(&mut decode_table_rows(&child, &None, context))
                            }
                            local_name!("tfoot") => rows.append(&mut decode_table_rows(
                                &child,
                                &Some(TableRowRowType::Footer),
                                context,
                            )),
                            local_name!("tr") => {
                                rows.push(decode_table_row(&child, &None, context))
                            }
                            _ => (),
                        };
                    }
                }
                vec![BlockContent::Table(TableSimple {
                    rows,
                    ..Default::default()
                })]
            }
            local_name!("hr") => {
                vec![BlockContent::ThematicBreak(ThematicBreak::default())]
            }

            // Recurse into HTML block elems thereby ignoring them but
            // not their children
            local_name!("html")
            | local_name!("body")
            | local_name!("article")
            | local_name!("main")
            | local_name!("aside")
            | local_name!("div")
            | local_name!("section") => decode_blocks(node, context),

            // All other elements (e.g. inlines) are skipped
            _ => vec![],
        }
    } else if let Some(text) = node.as_text() {
        // Decode HTML non-whitespace text by optionally parsing it to the `text_decoder` callback
        if !text.borrow().trim().is_empty() {
            if let Some(text_decoder) = &context.text_decoder {
                text_decoder(&text.borrow())
            } else {
                vec![BlockContent::Paragraph(Paragraph {
                    content: vec![InlineContent::String(text.borrow().clone())],
                    ..Default::default()
                })]
            }
        } else {
            vec![]
        }
    } else {
        // Skip everything else
        vec![]
    }
}

/// Decode the children of a HTML node into a vector of `InlineContent`
fn decode_inlines(node: &NodeRef, context: &DecodeContext) -> Vec<InlineContent> {
    node.children()
        .flat_map(|child| decode_inline(&child, context))
        .collect()
}

/// Decode a HTML node into a zero or more `InlineContent` nodes.
///
/// This function should handle most of the HTML "Phrasing content"
/// [elements](https://developer.mozilla.org/en-US/docs/Web/Guide/HTML/Content_categories#phrasing_content)
fn decode_inline(node: &NodeRef, context: &DecodeContext) -> Vec<InlineContent> {
    if let Some(element) = node.as_element() {
        let tag = element.name.local.clone();
        // Decode a HTML element
        //
        // Custom elements must be dealt with outside of the following match.
        #[allow(clippy::cmp_owned)]
        if tag == LocalName::from("stencila-code-fragment")
            || tag == LocalName::from("stencila-code-expression")
        {
            let programming_language = element
                .attributes
                .borrow()
                .get(LocalName::from("programming-language"))
                .map(|value| value.to_string());

            let text = if let Ok(text) = node.select_first("[slot=text]") {
                collect_text(text.as_node())
            } else {
                "".to_string()
            };

            return if tag == LocalName::from("stencila-code-fragment") {
                vec![InlineContent::CodeFragment(CodeFragment {
                    programming_language: programming_language.map(Box::new),
                    text,
                    ..Default::default()
                })]
            } else {
                vec![InlineContent::CodeExpression(CodeExpression {
                    programming_language: programming_language.unwrap_or_default(),
                    text,
                    ..Default::default()
                })]
            };
        }
        // The following are ordered alphabetically by the output node type
        // with placeholder comments for types not implemented yet.
        match tag {
            local_name!("audio") => {
                let attrs = element.attributes.borrow();
                let content_url = attrs.get(local_name!("src")).unwrap_or("").to_string();

                vec![InlineContent::AudioObject(AudioObjectSimple {
                    content_url,
                    ..Default::default()
                })]
            }
            // TODO: Cite
            // TODO: CiteGroup
            local_name!("code") => {
                // See note for `CodeBlock` on choice of attribute for decoding `programming_language`
                let programming_language = element
                    .attributes
                    .borrow()
                    .get(local_name!("class"))
                    .map(|value| Box::new(value.replace("language-", "")));
                let text = collect_text(node);
                vec![InlineContent::CodeFragment(CodeFragment {
                    text,
                    programming_language,
                    ..Default::default()
                })]
            }
            local_name!("del") => {
                vec![InlineContent::Strikeout(Strikeout {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            local_name!("em") => {
                vec![InlineContent::Emphasis(Emphasis {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            local_name!("img") => {
                let attrs = element.attributes.borrow();
                let content_url = attrs.get(local_name!("src")).unwrap_or("").to_string();
                let caption = attrs
                    .get(local_name!("title"))
                    .map(|value| Box::new(value.to_string()));

                vec![InlineContent::ImageObject(ImageObjectSimple {
                    content_url,
                    caption,
                    ..Default::default()
                })]
            }
            local_name!("a") => {
                let attrs = element.attributes.borrow();
                let target = attrs.get(local_name!("href")).unwrap_or("").to_string();
                let title = attrs
                    .get(local_name!("title"))
                    .map(|value| Box::new(value.to_string()));
                let content = decode_inlines(node, context);

                vec![InlineContent::Link(Link {
                    content,
                    target,
                    title,

                    ..Default::default()
                })]
            }
            // TODO: MathFragment
            local_name!("u") => {
                vec![InlineContent::Underline(Underline {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            // TODO: Note
            local_name!("input") => {
                let attrs = element.attributes.borrow();
                let name = attrs.get(local_name!("name")).unwrap_or("").to_string();
                let validator = if let Some(type_) = attrs.get(local_name!("type")) {
                    let validator = match type_ {
                        "number" => ValidatorTypes::NumberValidator(NumberValidator {
                            ..Default::default()
                        }),
                        _ => ValidatorTypes::StringValidator(StringValidator::default()),
                    };
                    Some(Box::new(validator))
                } else {
                    None
                };
                let value = attrs
                    .get(local_name!("value"))
                    .and_then(|value| TxtCodec::from_str(value, None).ok())
                    .map(Box::new);

                vec![InlineContent::Parameter(Parameter {
                    name,
                    validator,
                    value,
                    ..Default::default()
                })]
            }
            local_name!("q") => {
                vec![InlineContent::Quote(Quote {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            local_name!("strong") => {
                vec![InlineContent::Strong(Strong {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            local_name!("sub") => {
                vec![InlineContent::Subscript(Subscript {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            local_name!("sup") => {
                vec![InlineContent::Superscript(Superscript {
                    content: decode_inlines(node, context),
                    ..Default::default()
                })]
            }
            local_name!("video") => {
                let (content_url, media_type) = if let Ok(source) = node.select_first("source") {
                    let attrs = source.attributes.borrow();
                    let content_url = attrs.get(local_name!("src")).unwrap_or("").to_string();
                    let media_type = attrs
                        .get(local_name!("type"))
                        .map(|value| Box::new(value.to_string()));
                    (content_url, media_type)
                } else {
                    let attrs = element.attributes.borrow();
                    let content_url = attrs.get(local_name!("src")).unwrap_or("").to_string();
                    (content_url, None)
                };

                vec![InlineContent::VideoObject(VideoObjectSimple {
                    content_url,
                    media_type,
                    ..Default::default()
                })]
            }

            // Recurse into all other elements thereby ignoring them but
            // not their inline children
            _ => decode_inlines(node, context),
        }
    } else if let Some(text) = node.as_text() {
        // Decode HTML text by optionally parsing it as a Markdown fragment
        // and unwrapping from `Vec<BlockContent>` to `Vec<InlineContent>`.
        if !text.borrow().is_empty() {
            if let Some(text_decoder) = &context.text_decoder {
                text_decoder(&text.borrow()).to_inlines()
            } else {
                vec![InlineContent::String(text.borrow().clone())]
            }
        } else {
            vec![]
        }
    } else {
        // Skip everything else
        vec![]
    }
}

/// Decode list items from a `<ul>` or `<ol>`.
///
/// Only `<li>` children (and their descendants) are returned.
fn decode_list_items(node: &NodeRef, context: &DecodeContext) -> Vec<ListItem> {
    node.children()
        .filter_map(|child| {
            if let Some(element) = child.as_element() {
                if matches!(element.name.local, local_name!("li")) {
                    let blocks = decode_blocks(&child, context);
                    let content = if !blocks.is_empty() {
                        Some(ListItemContent::VecBlockContent(blocks))
                    } else {
                        let inlines = decode_inlines(&child, context);
                        if !inlines.is_empty() {
                            Some(ListItemContent::VecInlineContent(inlines))
                        } else {
                            None
                        }
                    };

                    return Some(ListItem {
                        content,
                        ..Default::default()
                    });
                }
            }
            None
        })
        .collect()
}

/// Decode table rows from `<tr>` children elements.
fn decode_table_rows(
    node: &NodeRef,
    row_type: &Option<TableRowRowType>,
    context: &DecodeContext,
) -> Vec<TableRow> {
    node.children()
        .filter_map(|child| {
            if let Some(element) = child.as_element() {
                if matches!(element.name.local, local_name!("tr")) {
                    return Some(decode_table_row(&child, row_type, context));
                }
            }
            None
        })
        .collect()
}

/// Decode a table row from a `<tr>` element.
///
/// If all the cells in the row are of type `<th>` then set the row type
/// to `Header`.
fn decode_table_row(
    node: &NodeRef,
    row_type: &Option<TableRowRowType>,
    context: &DecodeContext,
) -> TableRow {
    let cells = decode_table_cells(node, context);

    let row_type = if row_type.is_some() {
        row_type.clone()
    } else {
        let mut cells_all_header: bool = true;
        for cell in &cells {
            if !matches!(cell.cell_type, Some(TableCellCellType::Header)) {
                cells_all_header = false;
                break;
            }
        }

        if cells_all_header {
            Some(TableRowRowType::Header)
        } else {
            None
        }
    };

    TableRow {
        row_type,
        cells,
        ..Default::default()
    }
}

/// Decode table cells from a `<td>` or `<th> elements.
fn decode_table_cells(node: &NodeRef, context: &DecodeContext) -> Vec<TableCell> {
    node.children()
        .filter_map(|child| {
            if let Some(element) = child.as_element() {
                if matches!(element.name.local, local_name!("td") | local_name!("th")) {
                    let cell_type = if matches!(element.name.local, local_name!("th")) {
                        Some(TableCellCellType::Header)
                    } else {
                        None
                    };

                    let blocks = decode_blocks(&child, context);
                    let content = if blocks.len() > 1 {
                        Some(TableCellContent::VecBlockContent(blocks))
                    } else if let Some(BlockContent::Paragraph(para)) = blocks.first() {
                        Some(TableCellContent::VecInlineContent(para.content.clone()))
                    } else {
                        let inlines = decode_inlines(&child, context);
                        if !inlines.is_empty() {
                            Some(TableCellContent::VecInlineContent(inlines))
                        } else {
                            None
                        }
                    };

                    return Some(TableCell {
                        cell_type,
                        content,
                        ..Default::default()
                    });
                }
            }
            None
        })
        .collect()
}

/// Get the `id` attribute of an element (if any)
#[allow(clippy::box_collection)]
fn get_id(element: &ElementData) -> Option<Box<String>> {
    element
        .attributes
        .borrow()
        .get(local_name!("id"))
        .map(|id| Box::new(id.to_string()))
}

/// Accumulate all the text within a node, including text within descendant elements.
fn collect_text(node: &NodeRef) -> String {
    if let Some(text) = node.as_text() {
        text.borrow().to_string()
    } else {
        node.children().fold(String::new(), |acc, child| {
            [acc, collect_text(&child)].concat()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_snaps::{insta::assert_json_snapshot, snapshot_fixtures_content};

    #[test]
    fn decode_html_articles() {
        snapshot_fixtures_content("articles/*.html", |content| {
            assert_json_snapshot!(decode(content, None).expect("Unable to decode HTML"));
        });
    }

    #[test]
    fn decode_html_fragments() {
        snapshot_fixtures_content("fragments/html/*.html", |content| {
            assert_json_snapshot!(decode_fragment(content, None));
        });
    }
}
