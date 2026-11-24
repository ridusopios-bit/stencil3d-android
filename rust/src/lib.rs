pub mod mesh3d;
pub mod processor;
pub mod step_writer;

use image::RgbaImage;
use jni::objects::{JByteArray, JClass, JString};
use jni::sys::{jbyteArray, jstring};
use jni::JNIEnv;
use processor::{decode_and_downscale, process_rgba, StencilModel, StencilParams};
use std::sync::{Mutex, OnceLock};

struct AppState {
    image: Option<RgbaImage>,
    last_model: Option<StencilModel>,
    last_params: StencilParams,
}

fn state() -> &'static Mutex<AppState> {
    static STATE: OnceLock<Mutex<AppState>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(AppState {
            image: None,
            last_model: None,
            last_params: StencilParams::default(),
        })
    })
}

fn to_jstring(env: &mut JNIEnv, s: &str) -> jstring {
    env.new_string(s)
        .map(|js| js.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

fn to_jbytes(env: &mut JNIEnv, data: &[u8]) -> jbyteArray {
    env.byte_array_from_slice(data)
        .map(|arr| arr.into_raw())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub extern "system" fn Java_com_stencil3d_app_NativeBridge_setImage<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    image_bytes: JByteArray<'local>,
) -> jstring {
    let bytes = match env.convert_byte_array(&image_bytes) {
        Ok(b) => b,
        Err(e) => return to_jstring(&mut env, &format!("ERR:{e}")),
    };
    match decode_and_downscale(&bytes, 720) {
        Ok(img) => {
            if let Ok(mut st) = state().lock() {
                st.image = Some(img);
                st.last_model = None;
            }
            to_jstring(&mut env, "OK")
        }
        Err(e) => to_jstring(&mut env, &format!("ERR:{e}")),
    }
}

#[no_mangle]
pub extern "system" fn Java_com_stencil3d_app_NativeBridge_processCached<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
    params_json: JString<'local>,
) -> jstring {
    let p_str: String = match env.get_string(&params_json) {
        Ok(s) => s.into(),
        Err(e) => return to_jstring(&mut env, &format!("{{\"error\":\"{e}\"}}")),
    };
    let params: StencilParams = serde_json::from_str(&p_str).unwrap_or_default();

    let img_opt = state().lock().ok().and_then(|st| st.image.clone());
    let Some(img) = img_opt else {
        return to_jstring(&mut env, "{\"error\":\"Сначала выберите изображение\"}");
    };

    match process_rgba(&img, &params) {
        Ok(model) => {
            let json = serde_json::to_string(&model)
                .unwrap_or_else(|e| format!("{{\"error\":\"{e}\"}}"));
            if let Ok(mut st) = state().lock() {
                st.last_model = Some(model);
                st.last_params = params;
            }
            to_jstring(&mut env, &json)
        }
        Err(e) => {
            let err_json = serde_json::json!({ "error": e }).to_string();
            to_jstring(&mut env, &err_json)
        }
    }
}

#[no_mangle]
pub extern "system" fn Java_com_stencil3d_app_NativeBridge_getMesh3d<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jbyteArray {
    let snap = state()
        .lock()
        .ok()
        .and_then(|st| st.last_model.clone().map(|m| (m, st.last_params.clone())));
    let Some((model, params)) = snap else {
        return to_jbytes(&mut env, &[]);
    };
    let mesh = mesh3d::build_mesh3d(&model, &params);
    let packed = mesh3d::pack_mesh_bytes(&mesh);
    to_jbytes(&mut env, &packed)
}

#[no_mangle]
pub extern "system" fn Java_com_stencil3d_app_NativeBridge_exportStep<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jbyteArray {
    let snap = state()
        .lock()
        .ok()
        .and_then(|st| st.last_model.clone().map(|m| (m, st.last_params.clone())));
    let Some((model, params)) = snap else {
        return to_jbytes(&mut env, &[]);
    };
    let data = step_writer::generate_step(&model, &params);
    to_jbytes(&mut env, &data)
}

#[no_mangle]
pub extern "system" fn Java_com_stencil3d_app_NativeBridge_exportStl<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jbyteArray {
    let snap = state()
        .lock()
        .ok()
        .and_then(|st| st.last_model.clone().map(|m| (m, st.last_params.clone())));
    let Some((model, params)) = snap else {
        return to_jbytes(&mut env, &[]);
    };
    let mesh = mesh3d::build_mesh3d(&model, &params);
    let data = mesh3d::generate_binary_stl(&mesh);
    to_jbytes(&mut env, &data)
}

#[no_mangle]
pub extern "system" fn Java_com_stencil3d_app_NativeBridge_exportSvg<'local>(
    mut env: JNIEnv<'local>,
    _class: JClass<'local>,
) -> jbyteArray {
    let model = state().lock().ok().and_then(|st| st.last_model.clone());
    let Some(m) = model else {
        return to_jbytes(&mut env, &[]);
    };
    let data = step_writer::generate_svg(&m);
    to_jbytes(&mut env, &data)
}
