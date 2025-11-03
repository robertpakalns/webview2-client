fn main() {
    embed_resource::compile("./assets/manifest.rc", embed_resource::NONE)
        .manifest_required()
        .ok();
}
