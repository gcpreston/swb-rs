fn main() {
    #[cfg(windows)]
    {
        embed_resource::compile(
            "resources/windows/swb-gui.rc",
            embed_resource::NONE,
        ).unwrap();
    }
}
