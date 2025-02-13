use common::eyre::{bail, Result};
use node_address::Address;
use node_dispatch::dispatch_block;
use stencila_schema::*;

use crate::{Pointable, Pointer, PointerMut, Visitor, VisitorMut};

impl Pointable for BlockContent {
    /// Resolve an [`Address`] into a node [`Pointer`].
    ///
    /// `BlockContent` is one of the pointer variants so return a `Pointer::Block` if
    /// the address is empty. Otherwise dispatch to variant.
    fn resolve(&self, address: &mut Address) -> Result<Pointer> {
        match address.is_empty() {
            true => Ok(Pointer::Block(self)),
            false => dispatch_block!(self, resolve, address),
        }
    }
    fn resolve_mut(&mut self, address: &mut Address) -> Result<PointerMut> {
        match address.is_empty() {
            true => Ok(PointerMut::Block(self)),
            false => dispatch_block!(self, resolve_mut, address),
        }
    }

    /// Find a node based on its `id` and return a [`Pointer`] to it.
    ///
    /// Dispatch to variant and if it returns `Pointer::Some` then rewrite to `Pointer::Block`
    fn find(&self, id: &str) -> Pointer {
        match dispatch_block!(self, find, id) {
            Pointer::Some => Pointer::Block(self),
            _ => Pointer::None,
        }
    }
    fn find_mut(&mut self, id: &str) -> PointerMut {
        match dispatch_block!(self, find_mut, id) {
            PointerMut::Some => PointerMut::Block(self),
            _ => PointerMut::None,
        }
    }

    /// Walk over a node with a [`Visitor`]
    ///
    /// `BlockContent` is one of the visited types so call `visit_block` and,
    /// if it returns `true`, continue walk over variant.
    fn walk(&self, address: Address, visitor: &mut impl Visitor) {
        let cont = visitor.visit_block(&address, self);
        if cont {
            dispatch_block!(self, walk, address, visitor)
        }
    }
    fn walk_mut(&mut self, address: Address, visitor: &mut impl VisitorMut) {
        let cont = visitor.visit_block_mut(&address, self);
        if cont {
            dispatch_block!(self, walk_mut, address, visitor)
        }
    }
}

// Implementations for `BlockContent` structs (usually only properties that are content or `Node`s)
// and associated enums (only variants containing content).

pointable_struct!(Call, arguments, content);
pointable_struct!(CallArgument, default, validator, value);

pointable_struct!(ClaimSimple, content);
pointable_struct!(CodeBlock);

pointable_struct!(CodeChunk, caption, errors);
pointable_variants!(CodeChunkCaption, CodeChunkCaption::VecBlockContent);
pointable_struct!(CodeError);

pointable_struct!(CollectionSimple);
pointable_struct!(FigureSimple, caption);
pointable_variants!(FigureCaption, FigureCaption::VecBlockContent);
pointable_struct!(Heading, content);
pointable_struct!(Include, content);

pointable_struct!(List, items);
pointable_struct!(ListItem, content);
pointable_variants!(
    ListItemContent,
    ListItemContent::VecBlockContent,
    ListItemContent::VecInlineContent
);

pointable_struct!(MathBlock);
pointable_struct!(Paragraph, content);
pointable_struct!(QuoteBlock, content);

pointable_struct!(TableCell, content);
pointable_struct!(TableRow, cells);
pointable_struct!(TableSimple, caption, rows);
pointable_variants!(TableCaption, TableCaption::VecBlockContent);
pointable_variants!(
    TableCellContent,
    TableCellContent::VecBlockContent,
    TableCellContent::VecInlineContent
);

pointable_struct!(ThematicBreak);
