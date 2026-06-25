//! Reactive collection + conditional helpers: [`each`] and [`show`].
//!
//! Both build on [`dynamic`](crate::dynamic): they read signals to produce a
//! `BoxedView`, so the rendered list/branch rebuilds when the data changes.
//! These are the idiomatic "render a collection" and "conditionally render"
//! primitives (the For/Show of other frameworks).

use crate::container::column;
use crate::dynamic::{dynamic, Dynamic};
use crate::view::{boxed, BoxedView, View};

/// Renders one view per item in a reactive collection, stacked in a column.
/// Rebuilds whenever `items` (the signals it reads) changes.
///
/// ```
/// # use rax_view::each;
/// # use rax_reactive::create_signal;
/// let items = create_signal(vec![1, 2, 3]);
/// let _list = each(move || items.get(), |n| rax_view::boxed(rax_view::text(n.to_string())));
/// ```
pub fn each<T, I, F>(items: I, mut render: F) -> Dynamic<impl FnMut() -> BoxedView + 'static>
where
    T: 'static,
    I: Fn() -> Vec<T> + 'static,
    F: FnMut(T) -> BoxedView + 'static,
{
    dynamic(move || {
        let children: Vec<BoxedView> = items().into_iter().map(&mut render).collect();
        boxed(column(children))
    })
}

/// Renders `view` only when `cond` is true (an empty placeholder otherwise).
/// Rebuilds when `cond`'s signals change.
pub fn show<C, F, V>(cond: C, view: F) -> Dynamic<impl FnMut() -> BoxedView + 'static>
where
    C: Fn() -> bool + 'static,
    F: Fn() -> V + 'static,
    V: View + 'static,
{
    dynamic(move || {
        if cond() {
            boxed(view())
        } else {
            boxed(column(()))
        }
    })
}
