fn main() {
    println!("cargo:rerun-if-changed=../assets/logo/orbmc.ico");
    // Baked into the binary by `features::mods::curseforge_api_key`.
    println!("cargo:rerun-if-env-changed=CURSEFORGE_API_KEY");
    // Baked in by `features::rich_presence`.
    println!("cargo:rerun-if-env-changed=ORBMC_DISCORD_APP_ID");
    println!("cargo:rerun-if-env-changed=ORBMC_UPDATE_PUBLIC_KEY");

    #[cfg(target_os = "windows")]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("../assets/logo/orbmc.ico");
        resource.set("ProductName", "OrbMC");
        resource.set("FileDescription", "OrbMC");
        if let Err(error) = resource.compile() {
            println!("cargo:warning=failed to embed the application icon: {error}");
        }
    }
}
