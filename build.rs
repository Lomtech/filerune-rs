//! Hängt unter Windows Icon und Versionsangaben an die .exe, damit die Datei im
//! Explorer und in der Taskleiste die Rune trägt statt des Standardsymbols.
//! Auf allen anderen Systemen tut dieses Skript nichts.

fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rerun-if-changed=icon.ico");
        println!("cargo:rerun-if-changed=filerune.rc");
        embed_resource::compile("filerune.rc", embed_resource::NONE)
            .manifest_optional()
            .unwrap();
    }
}
