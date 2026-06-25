//! Core, dependency-free data structures shared across the `rax` UI framework.
//!
//! This crate is the leaf of the workspace dependency graph: it depends on
//! nothing (not even `std` in non-test builds) so that the geometry primitives
//! and the retained-tree storage are trivially unit-testable and portable.
//!
//! It deliberately takes **no** stance on threading, async, or rendering — those
//! concerns belong to higher crates (`rax-view`, `rax-runtime`, backends).

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

extern crate alloc;

pub mod arena;
pub mod color;
pub mod geometry;
pub mod style;

pub use arena::{Arena, Index};
pub use color::{Color, ColorScheme};
pub use geometry::{EdgeInsets, Point, Rect, Size};
pub use style::{
    AlignItems, Dimension, FlexDirection, FlexWrap, JustifyContent, LayoutStyle, Position,
};
