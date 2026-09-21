// SPDX-License-Identifier: MIT OR Apache-2.0
//! rust-i18n embeds locales/*.yml at compile time, but cargo does not track
//! those files as sources — force a rebuild whenever they change.
//! On Windows the binary embeds a longPathAware manifest so encoded
//! bucket names can exceed MAX_PATH (registry/GPO opt-in still noted
//! in the docs — both are required).

fn main() {
    println!("cargo:rerun-if-changed=locales");
    #[cfg(target_os = "windows")]
    {
        if let Err(e) = embed_manifest() {
            println!("cargo:warning=failed to embed longPathAware manifest: {e}");
        }
    }
}

#[cfg(target_os = "windows")]
fn embed_manifest() -> Result<(), String> {
    let mut res = winresource::WindowsResource::new();
    res.set_manifest(
        "<assembly xmlns=\"urn:schemas-microsoft-com:asm.v1\" manifestVersion=\"1.0\">\
         <trustInfo xmlns=\"urn:schemas-microsoft-com:asm.v3\">\
         <security><requestedPrivileges><requestedExecutionLevel level=\"asInvoker\"/>\
         </requestedPrivileges></security></trustInfo>\
         <asmv3:application xmlns:asmv3=\"urn:schemas-microsoft-com:asm.v3\">\
         <asmv3:windowsSettings>\
         <longPathAware xmlns=\"http://schemas.microsoft.com/SMI/2016/WindowsSettings\">true</longPathAware>\
         </asmv3:windowsSettings></asmv3:application></assembly>",
    );
    res.compile().map_err(|e| e.to_string())
}
