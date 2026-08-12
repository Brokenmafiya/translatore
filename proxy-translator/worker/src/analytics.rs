use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use wasm_bindgen::JsValue;
use worker::js_sys::Object;
use worker::*;

pub struct JsObjectBinding(pub Object);

impl EnvBinding for JsObjectBinding {
    const TYPE_NAME: &'static str = "Object";
    fn get(val: JsValue) -> Result<Self> {
        Ok(JsObjectBinding(Object::from(val)))
    }
}

impl JsCast for JsObjectBinding {
    fn instanceof(val: &JsValue) -> bool {
        val.is_object()
    }
    fn unchecked_from_js(val: JsValue) -> Self {
        JsObjectBinding(Object::from(val))
    }
    fn unchecked_from_js_ref(val: &JsValue) -> &Self {
        unsafe { &*(val as *const JsValue as *const Self) }
    }
}

impl AsRef<JsValue> for JsObjectBinding {
    fn as_ref(&self) -> &JsValue {
        self.0.as_ref()
    }
}

impl From<JsObjectBinding> for JsValue {
    fn from(val: JsObjectBinding) -> JsValue {
        val.0.into()
    }
}

/// Log metadata to Cloudflare Analytics Engine (no payloads, privacy-preserving)
pub fn log_traffic(env: &Env, device: &str, traffic_type: &str, target: &str, bytes: u64, status: u16) {
    if let Ok(dataset) = env.get_binding::<JsObjectBinding>("PROXY_METRICS") {
        let js_obj = &dataset.0;
        let point = serde_json::json!({
            "blobs": [device, traffic_type, target],
            "doubles": [bytes as f64],
            "indexes": [status.to_string()]
        });
        if let Ok(js_val) = serde_wasm_bindgen::to_value(&point) {
            let _ = worker::js_sys::Reflect::get(js_obj, &"writeDataPoint".into())
                .and_then(|f| f.dyn_into::<worker::js_sys::Function>())
                .and_then(|f| f.call1(js_obj, &js_val));
        }
    }
}
