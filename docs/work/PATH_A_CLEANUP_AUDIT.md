# Path A Cleanup Audit

## WPT Baseline

- Baseline commit: `d3495548b2b39dc2cdfc4452bd893a757390262f`
- Date: 2026-05-03
- Command attempted:
  `./mach test-wpt --always-succeed --log-raw /tmp/path_a_range_wpt_baseline_raw.log tests/wpt/tests/html/semantics/forms/the-input-element/range*.html tests/wpt/mozilla/tests/appearance/input-range.html`
- Result: not runnable in this checkout before implementation because `./mach test-wpt` reported `servo.command_base.BuildNotFound: No Servo binary found. Perhaps you forgot to run ./mach build?`
- Baseline pass rate: unavailable until a Servo binary is built. Final verification must compare against this same selected range-test set after the implementation build is available.

## Cleanup Table

| File:line | Added in commit | Stays / Goes / Verify | Reason |
|---|---:|---|---|
| `components/layout/stylesheets/servo.css:201-204` | `360f71fb8a7` | Stays | Independent range UA sizing fix: author `height` should not be blocked by a 16px floor. |
| `components/layout/stylesheets/servo.css:211-217` | `360f71fb8a7` | Goes | Track/fill absolute positioning is replaced by Path A frame-level geometry. |
| `components/layout/stylesheets/servo.css:219-222` | `360f71fb8a7` | Goes | Thumb absolute positioning is the old architecture that caused transform drift. |
| `components/layout/stylesheets/servo.css:263-267` | `360f71fb8a7`, `d3495548b2b` | Goes | `--su-thumb-*` and margin-top measurement variables are script-side positioning scars. |
| `components/layout/stylesheets/servo.css:268-269` | `360f71fb8a7` | Stays | Style/animation only; Path A should make transform effects correct. |
| `components/layout/stylesheets/servo.css:272-280` | `360f71fb8a7` | Stays | Hover/active styling remains valid; Path A changes geometry ownership. |
| `components/layout/stylesheets/servo.css:282-288` | `360f71fb8a7` | Stays | Disabled fill transparency avoids double track and is independent of positioning. |
| `components/layout/stylesheets/servo.css:288-305` | `360f71fb8a7` | Stays | Disabled thumb/cursor/hover suppression are independent UI behavior. |
| `components/script/dom/document/document_event_handler.rs:435-463` | `360f71fb8a7` | Stays | Generic pointer-capture hover reconciliation; explicitly orthogonal. |
| `components/script/dom/document/document_event_handler.rs:625-638` | `360f71fb8a7` | Stays | Keeps hover tied to real hit-test target during capture; explicitly orthogonal. |
| `components/script/dom/document/document_event_handler.rs:930-934` | `360f71fb8a7` | Stays | Button-event hover reconciliation is an engine fix, not range geometry. |
| `components/script/dom/html/input_element/range_input_type.rs` removed `force_thumb_hover()` | `360f71fb8a7` | Stays | Removing imperative thumb hover is correct after hover reconciliation. |
| `components/script/dom/html/input_element/range_input_type.rs` removed `self.force_thumb_hover()` | `360f71fb8a7` | Stays | Same as above; no slider-specific hover workaround remains. |
| `components/script/dom/html/input_element/range_input_type.rs:509-519` | `360f71fb8a7`, `d3495548b2b` | Goes | Comment documents the temporary script measurement bridge that Path A removes. |
| `components/script/dom/html/input_element/range_input_type.rs:519-543` | `360f71fb8a7`, `d3495548b2b` | Goes | Script-side default/measured thumb geometry writes are replaced by layout. |
| `components/script/dom/html/input_element/input_type.rs:379-386` | `2f979c49b32` | Stays | Range keyboard hook is HTML behavior, not positioning. |
| `components/script/dom/html/input_element/mod.rs:2294-2304` | `2f979c49b32` | Stays | Dispatching range keydown to the input type is independent of geometry. |
| `components/script/dom/html/input_element/range_input_type.rs:9` | `2f979c49b32` | Stays | Keyboard enum imports are used by the range keyboard handler. |
| `components/script/dom/html/input_element/range_input_type.rs:271-311` | `2f979c49b32` | Stays | Arrow/Page/Home/End behavior remains in scope as an orthogonal fix. |
| `components/script/dom/html/input_element/range_input_type.rs:6` | `d3495548b2b` | Goes | `Au` import is only for the measurement workaround. |
| `components/script/dom/html/input_element/range_input_type.rs:22` | `d3495548b2b` | Goes | `Trusted` import is only for the queued measurement task. |
| `components/script/dom/html/input_element/range_input_type.rs:28` | `d3495548b2b` | Goes | `InputType` import is only for the queued measurement task. |
| `components/script/dom/html/input_element/range_input_type.rs:408` | `d3495548b2b` | Goes | `measurement_task_queued` is post-layout script measurement state. |
| `components/script/dom/html/input_element/range_input_type.rs:414` | `d3495548b2b` | Goes | `DEFAULT_RANGE_THUMB_SIZE_PX` is a magic fallback for script positioning. |
| `components/script/dom/html/input_element/range_input_type.rs:474` | `d3495548b2b` | Goes | Initializer for removed measurement state. |
| `components/script/dom/html/input_element/range_input_type.rs:481-490` | `d3495548b2b` | Goes | `update_with_measurement` wrapper/signature exists only for script measurement. |
| `components/script/dom/html/input_element/range_input_type.rs:520-560` | `d3495548b2b` | Goes | Reflow/measurement/queued task block is explicitly banned by Path A cleanup. |
| `components/script/dom/html/input_element/range_input_type.rs:563-592` | `d3495548b2b` | Goes | `update_thumb_and_fill_styles` performs the inline geometry writes that Path A removes. |
| `components/script/dom/html/input_element/range_input_type.rs:571-590` | `d3495548b2b` | Goes | Direct `style` attribute writes for thumb/fill geometry are banned. |
