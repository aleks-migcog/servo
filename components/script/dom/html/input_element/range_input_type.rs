/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */
use std::cell::{Cell, Ref};

use app_units::Au;
use html5ever::{local_name, ns};
use js::context::JSContext;
use keyboard_types::{Key, NamedKey};
use markup5ever::QualName;
use script_bindings::codegen::GenericBindings::HTMLInputElementBinding::HTMLInputElementMethods;
use script_bindings::codegen::GenericBindings::MouseEventBinding::MouseEventMethods;
use script_bindings::domstring::parse_floating_point_number;
use script_bindings::root::Dom;
use script_bindings::script_runtime::CanGc;
use style::selector_parser::PseudoElement;
use stylo_atoms::atom;

use crate::dom::bindings::cell::DomRefCell;
use crate::dom::bindings::codegen::Bindings::NodeBinding::NodeMethods;
use crate::dom::bindings::inheritance::Castable;
use crate::dom::bindings::refcounted::Trusted;
use crate::dom::bindings::str::DOMString;
use crate::dom::element::{CustomElementCreationMode, Element, ElementCreator};
use crate::dom::event::Event;
use crate::dom::input_element::HTMLInputElement;
use crate::dom::input_element::input_type::{InputType, InputUserEventResult, SpecificInputType};
use crate::dom::node::{Node, NodeTraits};
use crate::dom::types::MouseEvent;

#[derive(Clone, Copy, Default, JSTraceable, MallocSizeOf, PartialEq)]
enum DragState {
    #[default]
    Idle,
    Dragging {
        start_value: f64,
        focused_value: f64,
    },
}

#[derive(Default, JSTraceable, MallocSizeOf, PartialEq)]
#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
pub(crate) struct RangeInputType {
    shadow_tree: DomRefCell<Option<RangeInputShadowTree>>,
    drag_state: Cell<DragState>,
}

impl RangeInputType {
    /// Get the shadow tree for this [`HTMLInputElement`], if it is created and valid, otherwise
    /// recreate the shadow tree and return it.
    fn get_or_create_shadow_tree(
        &self,
        cx: &mut JSContext,
        input: &HTMLInputElement,
    ) -> Ref<'_, RangeInputShadowTree> {
        {
            if let Ok(shadow_tree) = Ref::filter_map(self.shadow_tree.borrow(), |shadow_tree| {
                shadow_tree.as_ref()
            }) {
                return shadow_tree;
            }
        }

        let element = input.upcast::<Element>();
        let shadow_root = element
            .shadow_root()
            .unwrap_or_else(|| element.attach_ua_shadow_root(cx, true));
        let shadow_root = shadow_root.upcast();
        *self.shadow_tree.borrow_mut() = Some(RangeInputShadowTree::new(cx, shadow_root));
        self.get_or_create_shadow_tree(cx, input)
    }

    fn thumb_width(&self) -> f64 {
        // `Node::border_box()` gives us a viewport-relative border-box rect
        // in `Au`, matching ClientX-relative math in `compute_value_at_event_point`.
        self.shadow_tree
            .borrow()
            .as_ref()
            .and_then(|tree| tree.slider_thumb.upcast::<Node>().border_box())
            .map(|rect| rect.size.width.to_f64_px())
            .unwrap_or(0.0)
    }
}

impl SpecificInputType for RangeInputType {
    /// <https://html.spec.whatwg.org/multipage/#range-state-(type=range):value-sanitization-algorithm>
    fn sanitize_value(&self, input: &HTMLInputElement, value: &mut DOMString) {
        if !value.is_valid_floating_point_number_string() {
            *value = DOMString::from(input.default_range_value().to_string());
        }
        if let Ok(fval) = &value.parse::<f64>() {
            let mut fval = *fval;
            // comparing max first, because if they contradict
            // the spec wants min to be the one that applies
            if let Some(max) = input.maximum() {
                if fval > max {
                    fval = max;
                }
            }
            if let Some(min) = input.minimum() {
                if fval < min {
                    fval = min;
                }
            }
            // https://html.spec.whatwg.org/multipage/#range-state-(type=range):suffering-from-a-step-mismatch
            // Spec does not describe this in a way that lends itself to
            // reproducible handling of floating-point rounding;
            // Servo may fail a WPT test because .1 * 6 == 6.000000000000001
            let mut step_for_format: Option<f64> = None;
            if let Some(allowed_value_step) = input.allowed_value_step() {
                step_for_format = Some(allowed_value_step);
                let step_base = input.step_base();
                let steps_from_base = (fval - step_base) / allowed_value_step;
                if steps_from_base.fract() != 0.0 {
                    // not an integer number of steps, there's a mismatch
                    // round the number of steps...
                    let int_steps = round_halves_positive(steps_from_base);
                    // and snap the value to that rounded value...
                    fval = int_steps * allowed_value_step + step_base;

                    // but if after snapping we're now outside min..max
                    // we have to adjust! (adjusting to min last because
                    // that "wins" over max in the spec)
                    if let Some(stepped_maximum) = input.stepped_maximum() {
                        if fval > stepped_maximum {
                            fval = stepped_maximum;
                        }
                    }
                    if let Some(stepped_minimum) = input.stepped_minimum() {
                        if fval < stepped_minimum {
                            fval = stepped_minimum;
                        }
                    }
                }
            }
            // Round to step's decimal precision to mask f64 ULP jitter
            // introduced by `int_steps * step + base` arithmetic above.
            // Firefox / Chrome use Decimal arithmetic for this; Servo lacks
            // it, so a value of e.g. 1.65 (step 0.01) ends up as
            // 1.6500000000000001 in f64 - the user-visible string then
            // diverges from the reference browsers. Snap to step decimals
            // to match `<input type=range>.value` semantics across engines.
            if let Some(step) = step_for_format {
                if step > 0.0 && step.is_finite() {
                    let decimals = step_decimal_places(step);
                    if decimals < 16 {
                        let factor = 10f64.powi(decimals as i32);
                        fval = (fval * factor).round() / factor;
                    }
                }
            }
            *value = DOMString::from(fval.to_string());
        };
    }

    /// <https://html.spec.whatwg.org/multipage/#range-state-(type=range):concept-input-value-string-number>
    fn convert_string_to_number(&self, input: &str) -> Option<f64> {
        parse_floating_point_number(input)
    }

    /// <https://html.spec.whatwg.org/multipage/#range-state-(type=range):concept-input-value-string-number>
    fn convert_number_to_string(&self, input: f64) -> Option<DOMString> {
        let mut value = DOMString::from(input.to_string());
        value.set_best_representation_of_the_floating_point_number();
        Some(value)
    }

    /// <https://html.spec.whatwg.org/multipage/#range-state-(type=range):suffering-from-bad-input>
    fn suffers_from_bad_input(&self, value: &DOMString) -> bool {
        !value.is_valid_floating_point_number_string()
    }

    fn update_shadow_tree(&self, cx: &mut JSContext, input: &HTMLInputElement) {
        self.get_or_create_shadow_tree(cx, input).update(cx, input)
    }

    fn handle_mouse_event(
        &self,
        input: &HTMLInputElement,
        mouse_event: &MouseEvent,
        can_gc: CanGc,
    ) -> InputUserEventResult {
        // NOTE: do not dispatch `input`/`change` events from this method - the
        // caller in HTMLInputElement::handle_mouse_event holds a `Ref<InputType>`
        // borrow across the call. Synchronously running author script could
        // re-enter `attribute_mutated` and panic on `input_type.borrow_mut()`.
        // Report events via the returned `InputUserEventResult`; the caller
        // will dispatch them after dropping the borrow.
        let event = mouse_event.upcast::<Event>();
        let event_type = event.type_();
        let mut result = InputUserEventResult::handled();

        if event_type == atom!("mousedown") {
            // Primary button only, no Cmd/Ctrl modifier (Gecko convention).
            if mouse_event.Button() != 0 || mouse_event.CtrlKey() || mouse_event.MetaKey() {
                return result;
            }

            let element = input.upcast::<Element>();
            if element.disabled_state() {
                return result;
            }

            let start_value = input.ValueAsNumber();
            self.drag_state.set(DragState::Dragging {
                start_value,
                focused_value: start_value,
            });
            input
                .owner_document()
                .event_handler()
                .set_captured_range_input(Some(input));

            if let Some(new_value) =
                compute_value_at_event_point(input, mouse_event, self.thumb_width())
            {
                if set_range_value_for_user_event(input, new_value, can_gc) {
                    result.fire_input = true;
                }
            }
            return result;
        }

        if event_type == atom!("mousemove") {
            if !matches!(self.drag_state.get(), DragState::Dragging { .. }) {
                return result;
            }
            if let Some(new_value) =
                compute_value_at_event_point(input, mouse_event, self.thumb_width())
            {
                if set_range_value_for_user_event(input, new_value, can_gc) {
                    result.fire_input = true;
                }
            }
            return result;
        }

        if event_type == atom!("mouseup") {
            let DragState::Dragging { focused_value, .. } = self.drag_state.get() else {
                return result;
            };
            if let Some(new_value) =
                compute_value_at_event_point(input, mouse_event, self.thumb_width())
            {
                if set_range_value_for_user_event(input, new_value, can_gc) {
                    result.fire_input = true;
                }
            }
            input
                .owner_document()
                .event_handler()
                .set_captured_range_input(None);
            self.drag_state.set(DragState::Idle);
            if input.ValueAsNumber() != focused_value {
                result.fire_change = true;
            }
            return result;
        }

        result
    }

    fn cancel_range_drag(
        &self,
        input: &HTMLInputElement,
        can_gc: CanGc,
    ) -> InputUserEventResult {
        let DragState::Dragging { start_value, .. } = self.drag_state.get() else {
            return InputUserEventResult::unhandled();
        };

        self.drag_state.set(DragState::Idle);
        let mut result = InputUserEventResult::handled();
        if set_range_value_for_user_event(input, start_value, can_gc) {
            result.fire_input = true;
        }
        result
    }

    fn handle_keydown_event(
        &self,
        input: &HTMLInputElement,
        keyboard_event: &crate::dom::types::KeyboardEvent,
        can_gc: CanGc,
    ) -> InputUserEventResult {
        // See `handle_mouse_event`: do not dispatch events here, the caller
        // holds a `Ref<InputType>` borrow across this call.
        if input.upcast::<Element>().disabled_state() {
            return InputUserEventResult::unhandled();
        }

        let old_value = input.ValueAsNumber();
        let handled = match keyboard_event.key() {
            Key::Named(NamedKey::ArrowDown) | Key::Named(NamedKey::ArrowLeft) => {
                input.StepDown(1, can_gc).is_ok()
            },
            Key::Named(NamedKey::ArrowUp) | Key::Named(NamedKey::ArrowRight) => {
                input.StepUp(1, can_gc).is_ok()
            },
            Key::Named(NamedKey::PageDown) => input.StepDown(10, can_gc).is_ok(),
            Key::Named(NamedKey::PageUp) => input.StepUp(10, can_gc).is_ok(),
            Key::Named(NamedKey::Home) => input
                .minimum()
                .is_some_and(|minimum| input.SetValueAsNumber(minimum, can_gc).is_ok()),
            Key::Named(NamedKey::End) => input
                .maximum()
                .is_some_and(|maximum| input.SetValueAsNumber(maximum, can_gc).is_ok()),
            _ => false,
        };

        if !handled {
            return InputUserEventResult::unhandled();
        }

        let mut result = InputUserEventResult::handled();
        if input.ValueAsNumber() != old_value {
            result.fire_input = true;
            result.fire_change = true;
        }
        result
    }
}

/// SLIDER_PLAN P3 - port of Gecko's `nsRangeFrame::GetValueAtEventPoint`,
/// horizontal LTR only for now. Vertical and RTL layouts are followups.
///
/// Mirrors the thumb-center compensation from Gecko: the thumb's center
/// is offset from the input's content edge by `thumb_width / 2`, so the
/// reachable value range maps to `[thumb_width/2, width - thumb_width/2]`
/// in element-local x. Clicks to the left of that band yield `min`,
/// clicks to the right yield `max`.
///
/// Returns `None` when min/max are unset, the range is degenerate, or the
/// element has no traversable width.
fn compute_value_at_event_point(
    input: &HTMLInputElement,
    mouse_event: &MouseEvent,
    thumb_width: f64,
) -> Option<f64> {
    let min = input.minimum()?;
    let max = input.maximum()?;
    if max <= min {
        return Some(min);
    }

    // Viewport-relative border-box of the input. ClientX is also in
    // viewport CSS pixels, so subtraction yields element-local x.
    let rect = input.upcast::<Node>().border_box()?;
    let origin_x = rect.origin.x.to_f64_px();
    let width = rect.size.width.to_f64_px();
    if width <= 0.0 {
        return Some(min);
    }

    let traversable = (width - thumb_width).max(0.0);
    if traversable <= 0.0 {
        return Some(min);
    }

    let pos_at_start = thumb_width / 2.0;
    let pos_at_end = pos_at_start + traversable;
    let p = mouse_event.ClientX() as f64 - origin_x;
    let p_clamped = p.clamp(pos_at_start, pos_at_end);
    let fraction = (p_clamped - pos_at_start) / traversable;
    Some(min + fraction * (max - min))
}

fn set_range_value_for_user_event(input: &HTMLInputElement, value: f64, can_gc: CanGc) -> bool {
    let old_value = input.Value();
    if input.SetValueAsNumber(value, can_gc).is_err() {
        return false;
    }
    input.Value() != old_value
}

fn round_halves_positive(n: f64) -> f64 {
    // WHATWG specs about input steps say to round to the nearest step,
    // rounding halves always to positive infinity.
    // This differs from Rust's .round() in the case of -X.5.
    if n.fract() == -0.5 {
        n.ceil()
    } else {
        n.round()
    }
}

/// Count decimal places in `step`'s shortest-roundtrip representation.
/// Used to round the post-snap f64 to the precision the author asked for,
/// so e.g. step=0.01 produces "1.65" instead of "1.6500000000000001".
fn step_decimal_places(step: f64) -> usize {
    let s = step.abs().to_string();
    match s.find('.') {
        Some(idx) => s.len() - idx - 1,
        None => 0,
    }
}

#[derive(Clone, JSTraceable, MallocSizeOf, PartialEq)]
#[cfg_attr(crown, crown::unrooted_must_root_lint::must_root)]
/// Contains references to the elements in the shadow tree for `<input type=range>`.
///
/// The shadow tree consists of three div in the following structure:
/// <input type=range>
/// ├─ ::slider-track
/// │ └─ ::slider-fill
/// └─ ::slider-thumb
pub(crate) struct RangeInputShadowTree {
    measurement_task_queued: Cell<bool>,
    slider_fill: Dom<Element>,
    slider_thumb: Dom<Element>,
    slider_track: Dom<Element>,
}

const DEFAULT_RANGE_THUMB_SIZE_PX: f64 = 18.0;

impl RangeInputShadowTree {
    pub(crate) fn new(cx: &mut JSContext, shadow_root: &Node) -> Self {
        Node::replace_all(cx, None, shadow_root.upcast::<Node>());

        let slider_fill = Element::create(
            cx,
            QualName::new(None, ns!(html), local_name!("div")),
            None,
            &shadow_root.owner_document(),
            ElementCreator::ScriptCreated,
            CustomElementCreationMode::Asynchronous,
            None,
        );

        let slider_thumb = Element::create(
            cx,
            QualName::new(None, ns!(html), local_name!("div")),
            None,
            &shadow_root.owner_document(),
            ElementCreator::ScriptCreated,
            CustomElementCreationMode::Asynchronous,
            None,
        );

        let slider_track = Element::create(
            cx,
            QualName::new(None, ns!(html), local_name!("div")),
            None,
            &shadow_root.owner_document(),
            ElementCreator::ScriptCreated,
            CustomElementCreationMode::Asynchronous,
            None,
        );

        shadow_root
            .upcast::<Node>()
            .AppendChild(cx, slider_track.upcast::<Node>())
            .unwrap();
        slider_track
            .upcast::<Node>()
            .AppendChild(cx, slider_fill.upcast::<Node>())
            .unwrap();
        shadow_root
            .upcast::<Node>()
            .AppendChild(cx, slider_thumb.upcast::<Node>())
            .unwrap();

        slider_fill
            .upcast::<Node>()
            .set_implemented_pseudo_element(PseudoElement::SliderFill);
        slider_thumb
            .upcast::<Node>()
            .set_implemented_pseudo_element(PseudoElement::SliderThumb);
        slider_track
            .upcast::<Node>()
            .set_implemented_pseudo_element(PseudoElement::SliderTrack);

        Self {
            measurement_task_queued: Cell::new(false),
            slider_fill: slider_fill.as_traced(),
            slider_thumb: slider_thumb.as_traced(),
            slider_track: slider_track.as_traced(),
        }
    }

    pub(crate) fn update(&self, cx: &mut JSContext, input_element: &HTMLInputElement) {
        self.update_with_measurement(cx, input_element, false);
    }

    fn update_with_measurement(
        &self,
        cx: &mut JSContext,
        input_element: &HTMLInputElement,
        allow_reflow: bool,
    ) {
        let value = input_element.Value();
        let min = input_element
            .minimum()
            .expect("This value should be available for range input.");
        let max = input_element
            .maximum()
            .expect("This value should be available for range input.");
        let value_num = input_element
            .convert_string_to_number(&value.str())
            .unwrap_or(input_element.default_range_value());

        let percent = if min > max || (max - min).abs() < f64::EPSILON {
            0.0
        } else {
            let clamped_value = value_num.clamp(min, max);
            (clamped_value - min) / (max - min) * 100.0
        };

        // Position the thumb without using `transform`; author hover styles
        // commonly set `transform: scale(...)` and must not break centering.
        //
        // NOTE: Gecko does this at frame level in `nsRangeFrame::
        // DoUpdateThumbPosition`, after the anonymous thumb frame is reflowed.
        // Servo has no equivalent range-specific shadow-content reflow hook yet.
        // As a pragmatic bridge, write default variables, then measure the
        // resolved thumb border-box once the DOM is stable. This lets author
        // `::slider-thumb` / prefixed thumb width and height overrides
        // participate in fraction positioning.
        let fraction = percent / 100.0;
        self.update_thumb_and_fill_styles(
            percent,
            fraction,
            DEFAULT_RANGE_THUMB_SIZE_PX,
            DEFAULT_RANGE_THUMB_SIZE_PX,
            cx,
        );

        let thumb_node = self.slider_thumb.upcast::<Node>();
        let thumb_size = if allow_reflow {
            thumb_node.border_box()
        } else {
            thumb_node.border_box_without_reflow()
        }
        .map(|rect| rect.size)
        .filter(|size| size.width > Au::new(0) && size.height > Au::new(0));

        if let Some(size) = thumb_size {
            let thumb_width = size.width.to_f64_px();
            let thumb_height = size.height.to_f64_px();
            if (thumb_width - DEFAULT_RANGE_THUMB_SIZE_PX).abs() > f64::EPSILON
                || (thumb_height - DEFAULT_RANGE_THUMB_SIZE_PX).abs() > f64::EPSILON
            {
                self.update_thumb_and_fill_styles(percent, fraction, thumb_width, thumb_height, cx);
            }
        } else if !allow_reflow && !self.measurement_task_queued.replace(true) {
            let input = Trusted::new(input_element);
            input_element
                .owner_global()
                .task_manager()
                .dom_manipulation_task_source()
                .queue(task!(measure_range_thumb: move |cx| {
                    let input = input.root();
                    let input_type = input.input_type();
                    if let InputType::Range(range) = &*input_type {
                        let shadow_tree = range.get_or_create_shadow_tree(cx, &input);
                        shadow_tree.measurement_task_queued.set(false);
                        shadow_tree.update_with_measurement(cx, &input, true);
                    }
                }));
        }
    }

    fn update_thumb_and_fill_styles(
        &self,
        percent: f64,
        fraction: f64,
        thumb_width: f64,
        thumb_height: f64,
        cx: &mut JSContext,
    ) {
        self.slider_thumb.set_string_attribute(
            &local_name!("style"),
            format!(
                "--su-thumb-w: {thumb_width}px; \
                 --su-thumb-h: {thumb_height}px; \
                 --su-thumb-margin-top: -{half_thumb_height}px; \
                 inset-inline-start: calc({percent}% - {fraction} * var(--su-thumb-w)) !important;",
                half_thumb_height = thumb_height / 2.0,
            )
            .into(),
            CanGc::from_cx(cx),
        );
        self.slider_fill.set_string_attribute(
            &local_name!("style"),
            format!(
                "width: calc({percent}% - {fraction} * {thumb_width}px + {half_thumb_width}px) !important;",
                half_thumb_width = thumb_width / 2.0,
            )
            .into(),
            CanGc::from_cx(cx),
        );
    }
}
