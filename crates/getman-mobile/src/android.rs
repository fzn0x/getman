//! Android's certificate store for HTTPS.
//!
//! Getman verifies certificates with rustls-platform-verifier, which on
//! Android asks the system's trust manager through a small Kotlin class
//! (see gen/android/app/build.gradle.kts). It needs the app's Java VM and
//! context first, or the first HTTPS request panics.

use jni::objects::JObject;

/// Hands the app's Java VM and context to rustls-platform-verifier.
pub(crate) fn use_the_system_certificates() -> Result<(), String> {
    let android = ndk_context::android_context();
    if android.vm().is_null() || android.context().is_null() {
        return Err("Android did not give Getman its Java context".into());
    }
    // SAFETY: tao records the process's Java VM and the application's
    // context, a global reference, before the app starts, and both stay
    // valid for as long as the process runs.
    #[allow(unsafe_code)]
    let vm = unsafe { jni::JavaVM::from_raw(android.vm().cast()) };
    vm.attach_current_thread(|env| {
        // SAFETY: as above, the context is a live global reference.
        #[allow(unsafe_code)]
        let context = unsafe { JObject::from_raw(env, android.context().cast()) };
        rustls_platform_verifier::android::init_with_env(env, context)
    })
    .map_err(|e: jni::errors::Error| format!("Could not reach Android's certificates: {e}"))
}
