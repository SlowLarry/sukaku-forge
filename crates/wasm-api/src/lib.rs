//! Browser WebAssembly owner for the transport-neutral application port.
//!
//! All parsing, validation and session behavior remain in
//! [`sukaku_forge_app::port::ApplicationPort`]. This crate contributes only
//! the JavaScript-visible lifetime and exact JSON forwarding boundary.

/// One browser-worker-owned Sukaku Forge application port.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
#[derive(Clone, Debug, Default)]
pub struct WasmApplicationPort {
    inner: sukaku_forge_app::port::ApplicationPort,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen::prelude::wasm_bindgen)]
impl WasmApplicationPort {
    /// Create one independent session owner.
    #[cfg_attr(
        target_arch = "wasm32",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Forward one protocol JSON request and return the exact response JSON.
    pub fn dispatch_json(&mut self, request: &str) -> String {
        self.inner.dispatch_json(request)
    }
}

#[cfg(test)]
mod tests {
    use super::WasmApplicationPort;

    #[test]
    fn wrapper_matches_the_domain_port_and_retains_session_state() {
        let create = r#"{"protocol_version":2,"request_id":1,"command":"create_session","puzzle":"12345678........................................................................."}"#;
        let next = r#"{"protocol_version":2,"request_id":2,"command":"next_hint","expected_revision":"0"}"#;
        let mut wrapper = WasmApplicationPort::new();
        let mut direct = sukaku_forge_app::port::ApplicationPort::new();

        assert_eq!(wrapper.dispatch_json(create), direct.dispatch_json(create));
        assert_eq!(wrapper.dispatch_json(next), direct.dispatch_json(next));
    }
}
