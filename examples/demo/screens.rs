//! Which screen the demo shows, as the state the rest of the demo runs in.

use bevy::{
    app::{App, Plugin},
    state::{app::AppExtStates, state::States},
};

use crate::{
    gameplay::{GameplayPlugin, Renderer},
    main_screen::MainScreenPlugin,
};

pub struct ScreenPlugin {
    pub renderer: Renderer,
}

impl Plugin for ScreenPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<Screen>();
        app.add_plugins((
            MainScreenPlugin,
            GameplayPlugin {
                renderer: self.renderer,
            },
        ));
    }
}

#[derive(States, Clone, Copy, Eq, PartialEq, Hash, Debug, Default)]
pub enum Screen {
    #[default]
    Main,
    Help,
    Gameplay,
}
