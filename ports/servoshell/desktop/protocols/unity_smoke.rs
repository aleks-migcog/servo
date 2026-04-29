/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/. */

// Smoke-only `unity://` handler for Spike 7. Always returns 503 with a
// JSON body that lets the test page confirm Servo routed the request
// here. Production handler lives in the servo-unity FFI dylib; this is
// a throwaway shim to verify scheme-registration in raw servoshell
// without needing a Unity dylib swap.

use std::future::Future;
use std::pin::Pin;

use headers::{ContentType, HeaderMapExt};
use servo::protocol_handler::{
    DoneChannel, FetchContext, HttpStatus, ProtocolHandler, Request, ResourceFetchTiming, Response,
    ResponseBody,
};

#[derive(Default)]
pub struct UnitySmokeProtocolHandler {}

impl ProtocolHandler for UnitySmokeProtocolHandler {
    fn load(
        &self,
        request: &mut Request,
        _done_chan: &mut DoneChannel,
        _context: &FetchContext,
    ) -> Pin<Box<dyn Future<Output = Response> + Send>> {
        let url = request.current_url();
        let body = format!(
            r#"{{"ok":false,"error":"unity_smoke_handler","scheme":"{}","path":"{}","query":{}}}"#,
            url.scheme(),
            url.path(),
            url.query()
                .map(|q| format!("\"{}\"", q.replace('"', "\\\"")))
                .unwrap_or_else(|| "null".into()),
        );

        let mut response = Response::new(url, ResourceFetchTiming::new(request.timing_type()));
        *response.body.lock() = ResponseBody::Done(body.into_bytes());
        response.headers.typed_insert(ContentType::json());
        response.status = HttpStatus::new_raw(503, b"smoke-stub".to_vec());

        Box::pin(std::future::ready(response))
    }

    fn is_fetchable(&self) -> bool {
        true
    }

    fn is_secure(&self) -> bool {
        true
    }
}
