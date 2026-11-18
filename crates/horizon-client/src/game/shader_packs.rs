//! The shader pack library, selection and settings behind the Shader Packs pages
//! (`horizon_client::shader_packs_ui`). A stub until the manager lands: the pages show an empty
//! library and their events are dropped.

use super::Game;
use horizon_client::shader_packs_ui::{ShaderPackEvent, ShaderPacksView};

impl Game<'_> {
    /// A Shader Packs page event (`options_ui::Event::ShaderPacks`), handled while the click
    /// that caused it is (the browser's file picker needs that).
    pub(crate) fn shader_pack_event(&mut self, ev: ShaderPackEvent) {
        let _ = ev;
    }
    /// What the pages show; set into `ui.options.shader_packs` every frame while one shows
    /// (`poll_control`, game/control.rs).
    pub(crate) fn shader_packs_view(&self) -> ShaderPacksView {
        ShaderPacksView::default()
    }
}
