use std::path::Path;

use parser_treesitter::{
    common::{eyre::Result, once_cell::sync::Lazy},
    formats::Format,
    graph_triples::{relations, resources, Resource, ResourceInfo},
    resource_info, Parser, ParserTrait, TreesitterParser,
};

/// Tree-sitter based parser for JavaScript
static PARSER_JS: Lazy<TreesitterParser> = Lazy::new(|| {
    TreesitterParser::new(
        tree_sitter_typescript::language_typescript(),
        parser_js::QUERY,
    )
});

/// Tree-sitter based parser for TypeScript
static PARSER_TS: Lazy<TreesitterParser> =
    Lazy::new(|| TreesitterParser::new(tree_sitter_typescript::language_typescript(), QUERY));

/// Tree-sitter AST query for TypeScript
///
/// These are query patterns that extend those for JavaScript defined
/// in `parser-js`.
const QUERY: &str = include_str!("query.scm");

/// A parser for TypeScript
pub struct TsParser {}

impl ParserTrait for TsParser {
    fn spec() -> Parser {
        Parser {
            language: Format::TypeScript.spec().title,
        }
    }

    fn parse(resource: Resource, path: &Path, code: &str) -> Result<ResourceInfo> {
        let code = code.as_bytes();
        let tree = PARSER_TS.parse(code);

        // Query the tree for typed patterns defined in this module
        let matches = PARSER_TS.query(code, &tree);
        let relations_typed = matches
            .iter()
            .filter_map(|(pattern, captures)| match pattern {
                0 => {
                    // Assigns a symbol at the top level of the module
                    let range = captures[0].range;
                    let name = captures[0].text.clone();
                    let type_annotation = captures[1].node;
                    let type_string = type_annotation
                        .named_child(0)
                        .and_then(|node| node.utf8_text(code).ok())
                        .unwrap_or_default();
                    let kind = match type_string {
                        "boolean" => "Boolean",
                        "number" => "Number",
                        "string" => "String",
                        "object" => "Object",
                        _ => {
                            if type_string.starts_with("Array<") {
                                "Array"
                            } else if type_string.starts_with("Record<string") {
                                "Object"
                            } else {
                                ""
                            }
                        }
                    };
                    Some((
                        relations::declares(range),
                        resources::symbol(path, &name, kind),
                    ))
                }
                _ => None,
            });

        // Query the tree for untyped patterns defined in the JavaScript module
        let matches = PARSER_JS.query(code, &tree);
        let relations_untyped = matches.iter().filter_map(|(pattern, capture)| {
            parser_js::handle_patterns(path, code, pattern, capture)
        });

        let relations = relations_typed.chain(relations_untyped).collect();

        let resource_info = resource_info(
            resource,
            path,
            &Self::spec().language,
            code,
            &tree,
            &["comment"],
            matches,
            0,
            relations,
        );
        Ok(resource_info)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_snaps::{insta::assert_json_snapshot, snapshot_fixtures};
    use test_utils::fixtures;

    #[test]
    fn parse_ts_fragments() {
        snapshot_fixtures("fragments/ts/*.ts", |path| {
            let code = std::fs::read_to_string(path).expect("Unable to read");
            let path = path.strip_prefix(fixtures()).expect("Unable to strip");
            let resource = resources::code(
                path,
                "",
                "SoftwareSourceCode",
                Some("TypeScript".to_string()),
            );
            let resource_info = TsParser::parse(resource, path, &code).expect("Unable to parse");
            assert_json_snapshot!(resource_info);
        })
    }

    #[test]
    fn parse_js_fragments() {
        snapshot_fixtures("fragments/js/*.js", |path| {
            let code = std::fs::read_to_string(path).expect("Unable to read");
            let path = path.strip_prefix(fixtures()).expect("Unable to strip");
            let resource = resources::code(
                path,
                "",
                "SoftwareSourceCode",
                Some("JavaScript".to_string()),
            );
            let resource_info = TsParser::parse(resource, path, &code).expect("Unable to parse");
            assert_json_snapshot!(resource_info);
        })
    }
}
