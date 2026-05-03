/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

//! Scrollbar gutter reservation for scroll containers.
//!
//! Servo does not paint classic (non-overlay) scrollbars itself yet
//! (`<https://github.com/servo/servo/issues/41341>`), but embedders such as
//! `servo-unity` overlay native-looking scrollbars on top of the rendered
//! surface. Without reserving inline-end / block-end space inside scroll
//! containers, content slides under the embedder-drawn scrollbar - the
//! `app_simple` DetailPanel symptom that motivated this module.
//!
//! This file is the single layout-side helper for that reservation. Each
//! formatting context that lays out a scroll container's children
//! (`flexbox`, `flow` / BFC, taffy-backed grid, table) is expected to:
//!
//! 1. Compute the gutter via [`scrollbar_gutter`] using the container's own
//!    style and the device-side scrollbar inline-size.
//! 2. Shrink the *child* containing block by that gutter, keeping the
//!    container's outer/border/content rect itself unchanged.
//!
//! The CSS source of truth for the size of a single classic scrollbar is
//! `Device::scrollbar_inline_size()` in stylo, which is also what backs the
//! `-moz-scrollbar-inline-size` environment variable. Having both layout
//! and CSS read the same number keeps author CSS using `env(...)` in sync
//! with what layout actually subtracts.

use app_units::Au;
use style::Zero;
use style::computed_values::overflow_x::T as Overflow;
use style::device::Device;
use style::properties::ComputedValues;

use crate::fragment_tree::FragmentFlags;
use crate::geom::LogicalSides;
use crate::style_ext::ComputedValuesExt;

/// How to treat `overflow: auto` when deciding whether to reserve a gutter.
///
/// Per CSS Overflow Module Level 3, `auto` only renders a scrollbar when
/// content actually overflows. Some formatting contexts can do a two-pass
/// probe and use [`ScrollbarAutoPolicy::OnlyIfOverflows`]; others still use
/// the pessimistic one-pass approximation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScrollbarAutoPolicy {
    /// Treat `Overflow::Auto` identically to `Overflow::Scroll` and always
    /// reserve a gutter. Pessimistic (loses a few CSS pixels of content
    /// width when no scrollbar is actually shown) but matches Host6's
    /// always-visible overlay model and avoids the extra layout pass.
    /// This is also the precedent already set by Servo's taffy bridge in
    /// `taffy/stylo_taffy/convert.rs`, which maps `auto` to taffy
    /// `Overflow::Scroll` for the same reason.
    TreatAsScroll,

    /// Do not reserve gutter space for `Overflow::Auto` during the initial
    /// layout probe. Callers that can detect real overflow may then rerun
    /// layout with [`ScrollbarAutoPolicy::TreatAsScroll`] only when needed.
    OnlyIfOverflows,
}

/// Compute the inline / block sides of a scroll container's *child*
/// containing block that should be shrunk to make room for classic
/// scrollbars. The returned [`LogicalSides<Au>`] is meant to be subtracted
/// from the inline / block sizes available to the container's children
/// (e.g. a flex container's main / cross size for its items) - it is NOT
/// extra padding on the container's own border or content rect.
///
/// Conventions:
///
/// - A vertical scrollbar (rendered when `overflow-y` is `Scroll`, or
///   `Auto` per `auto_policy`) consumes inline-end space.
/// - A horizontal scrollbar (rendered when `overflow-x` is `Scroll`, or
///   `Auto` per `auto_policy`) consumes block-end space.
/// - `inline-start` / `block-start` stay zero. CSS `scrollbar-gutter:
///   stable both-edges` would also reserve the opposite sides; we don't
///   honour that property yet (stylo parses it but no engine reads it).
///
/// Caveats:
///
/// - Vertical writing modes are not supported yet. `flow/mod.rs` asserts
///   horizontal-only writing modes today, so layering this on top of that
///   constraint keeps the helper aligned with the rest of layout. Once
///   writing-mode mapping lands, swap the inline/block dimensions to the
///   container's own writing mode before composing the result.
/// - The function does not consult `scrollbar-gutter` (parsed in stylo,
///   not yet read by layout) or `scrollbar-width: thin/none` (stylo sets
///   these but the size still comes from `Device::scrollbar_inline_size`).
pub(crate) fn scrollbar_gutter(
    style: &ComputedValues,
    fragment_flags: FragmentFlags,
    scrollbar_inline_size: Au,
    auto_policy: ScrollbarAutoPolicy,
) -> LogicalSides<Au> {
    if scrollbar_inline_size <= Au::zero() {
        return LogicalSides::default();
    }

    let overflow = style.effective_overflow(fragment_flags);
    let reserves_for = |axis_overflow: Overflow| match axis_overflow {
        Overflow::Scroll => true,
        Overflow::Auto => match auto_policy {
            ScrollbarAutoPolicy::TreatAsScroll => true,
            ScrollbarAutoPolicy::OnlyIfOverflows => false,
        },
        // `Visible`, `Hidden`, `Clip` never paint a classic scrollbar.
        _ => false,
    };

    LogicalSides {
        inline_start: Au::zero(),
        inline_end: if reserves_for(overflow.y) {
            scrollbar_inline_size
        } else {
            Au::zero()
        },
        block_start: Au::zero(),
        block_end: if reserves_for(overflow.x) {
            scrollbar_inline_size
        } else {
            Au::zero()
        },
    }
}

/// Convenience constructor for layout call sites that already know the
/// host's `Device` instance: reads `scrollbar_inline_size` off it and
/// converts CSS pixels to `Au`.
pub(crate) fn scrollbar_gutter_for_device(
    style: &ComputedValues,
    fragment_flags: FragmentFlags,
    device: &Device,
    auto_policy: ScrollbarAutoPolicy,
) -> LogicalSides<Au> {
    let inline_size = Au::from_f32_px(device.scrollbar_inline_size().px());
    scrollbar_gutter(style, fragment_flags, inline_size, auto_policy)
}
