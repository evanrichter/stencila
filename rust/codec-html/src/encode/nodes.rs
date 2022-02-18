//! Encode `Node` nodes to HTML

use super::{
    attr, attr_itemtype_str, elem, json, primitives::array_to_html, EncodeContext, ToHtml,
};
use node_dispatch::dispatch_node;
use stencila_schema::Node;

/// Encode a `Node` to HTML
///
/// All node types that have an `impl ToHtml` should be listed here. Not all node types
/// are supported, in which case this function returns HTML indicating that that is the case.
impl ToHtml for Node {
    fn to_html(&self, context: &EncodeContext) -> String {
        // Call `array_to_html` to avoid `Vec<Primitive>.to_html()` for arrays
        if let Node::Array(array) = self {
            return array_to_html(array, context);
        }

        // Previously, we said:
        //   Wrap strings in a `<pre>` with the `itemtype` attribute.
        //   This encoding will be used in places such as `CodeChunk.outputs`, `CodeExpression.output` etc
        //   where pre-formatting is important and wrapping in an element is needed for patches (whitespace
        //   can be lost if not wrapped in a <pre>).
        // But..that caused problems for `CodeExpression.output` rendering. So here, it is changed back to <span>
        // and we'll see if we can find another way of dealing with the above issue.
        //
        // See note under `ToHtml` for `InlineContent` for how strings are handled in that context.
        if let Node::String(string) = self {
            return elem(
                "span",
                &[attr_itemtype_str("String")],
                &string.to_html(context),
            );
        }

        // Fallback to default `to_html` for other `Node` variants
        dispatch_node!(
            self,
            elem("div", &[attr("class", "unsupported")], &json(self)),
            to_html,
            context
        )
    }
}
