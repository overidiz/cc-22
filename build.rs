fn main() {
    println!("cargo:rerun-if-changed=assets/cc22.ico");
    #[cfg(target_os = "windows")]
    {
        let mut resource = winresource::WindowsResource::new();
        resource.set_icon("assets/cc22.ico");
        resource
            .compile()
            .expect("failed to embed Windows executable resources");
    }
}
