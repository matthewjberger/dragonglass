use anyhow::{Context, Result};

pub trait State<T, E> {
    fn label(&self) -> String {
        "Unlabeled Game State".to_string()
    }
    fn on_start(&mut self, _data: StateData<'_, T>) {}
    fn on_stop(&mut self, _data: StateData<'_, T>) {}
    fn on_pause(&mut self, _data: StateData<'_, T>) {}
    fn on_resume(&mut self, _data: StateData<'_, T>) {}
    fn handle_event(&mut self, _data: StateData<'_, T>, _event: E) -> Transition<T, E> {
        Transition::None
    }
    fn update(&mut self, _data: StateData<'_, T>) -> Transition<T, E> {
        Transition::None
    }
}

pub struct StateData<'a, T> {
    pub data: &'a mut T,
}

impl<'a, T> StateData<'a, T> {
    pub fn new(data: &'a mut T) -> Self {
        Self { data }
    }
}

pub enum Transition<T, E> {
    None,
    Pop,
    Push(Box<dyn State<T, E>>),
    Switch(Box<dyn State<T, E>>),
    Quit,
}

pub struct StateMachine<'a, T, E> {
    running: bool,
    states: Vec<Box<dyn State<T, E> + 'a>>,
}

impl<'a, T, E> StateMachine<'a, T, E> {
    pub fn new(initial_state: impl State<T, E> + 'a) -> StateMachine<'a, T, E> {
        Self {
            running: false,
            states: vec![Box::new(initial_state)],
        }
    }

    pub fn active_state_label(&self) -> Option<String> {
        if !self.running {
            return None;
        }
        match self.states.last() {
            Some(state) => Some(state.label()),
            None => None,
        }
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn start(&mut self, data: StateData<'_, T>) -> Result<()> {
        if !self.running {
            let state = self
                .states
                .last_mut()
                .context("Tried to start state machine with no states present!")?;
            state.on_start(data);
            self.running = true;
        }
        Ok(())
    }

    pub fn handle_event(&mut self, data: StateData<'_, T>, event: E) {
        let StateData { data } = data;
        if self.running {
            let transition = match self.states.last_mut() {
                Some(state) => state.handle_event(StateData { data }, event),
                None => Transition::None,
            };
            self.transition(transition, StateData { data });
        }
    }

    pub fn update(&mut self, data: StateData<'_, T>) {
        let StateData { data } = data;
        if self.running {
            let trans = match self.states.last_mut() {
                Some(state) => state.update(StateData { data }),
                None => Transition::None,
            };
            self.transition(trans, StateData { data });
        }
    }

    pub fn transition(&mut self, request: Transition<T, E>, data: StateData<'_, T>) {
        if self.running {
            match request {
                Transition::None => (),
                Transition::Pop => self.pop(data),
                Transition::Push(state) => self.push(state, data),
                Transition::Switch(state) => self.switch(state, data),
                Transition::Quit => self.stop(data),
            }
        }
    }

    fn switch(&mut self, state: Box<dyn State<T, E>>, data: StateData<'_, T>) {
        if self.running {
            let StateData { data } = data;
            if let Some(mut state) = self.states.pop() {
                state.on_stop(StateData { data });
            }
            self.states.push(state);
            let new_state = self.states.last_mut().unwrap();
            new_state.on_start(StateData { data });
        }
    }

    fn push(&mut self, state: Box<dyn State<T, E>>, data: StateData<'_, T>) {
        if self.running {
            let StateData { data } = data;
            if let Some(state) = self.states.last_mut() {
                state.on_pause(StateData { data });
            }
            self.states.push(state);
            let new_state = self.states.last_mut().unwrap();
            new_state.on_start(StateData { data });
        }
    }

    fn pop(&mut self, data: StateData<'_, T>) {
        if self.running {
            let StateData { data } = data;
            if let Some(mut state) = self.states.pop() {
                state.on_stop(StateData { data });
            }
            if let Some(state) = self.states.last_mut() {
                state.on_resume(StateData { data });
            } else {
                self.running = false;
            }
        }
    }

    pub fn stop(&mut self, data: StateData<'_, T>) {
        if self.running {
            let StateData { data } = data;
            while let Some(mut state) = self.states.pop() {
                state.on_stop(StateData { data });
            }
            self.running = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IntroState {
        countdown: u8,
    }
    impl State<(), ()> for IntroState {
        fn label(&self) -> String {
            "Intro".to_string()
        }
        fn update(&mut self, _: StateData<'_, ()>) -> Transition<(), ()> {
            if self.countdown > 0 {
                self.countdown -= 1;
                Transition::None
            } else {
                Transition::Switch(Box::new(MainMenuState))
            }
        }
    }

    struct MainMenuState;
    impl State<(), ()> for MainMenuState {
        fn label(&self) -> String {
            "MainMenu".to_string()
        }
        fn update(&mut self, _: StateData<'_, ()>) -> Transition<(), ()> {
            Transition::Switch(Box::new(GameplayState {
                paused: false,
                finished: false,
            }))
        }
    }

    struct GameplayState {
        paused: bool,
        finished: bool,
    }
    impl State<(), ()> for GameplayState {
        fn label(&self) -> String {
            "Gameplay".to_string()
        }
        fn on_resume(&mut self, _: StateData<'_, ()>) {
            self.finished = true;
        }
        fn update(&mut self, _: StateData<'_, ()>) -> Transition<(), ()> {
            if self.finished {
                Transition::Push(Box::new(GameOverState { countdown: 8 }))
            } else if self.paused {
                Transition::Push(Box::new(PauseState))
            } else {
                self.paused = true;
                Transition::None
            }
        }
    }

    struct PauseState;
    impl State<(), ()> for PauseState {
        fn label(&self) -> String {
            "Pause".to_string()
        }
        fn update(&mut self, _: StateData<'_, ()>) -> Transition<(), ()> {
            Transition::Pop
        }
    }

    struct GameOverState {
        countdown: u8,
    }
    impl State<(), ()> for GameOverState {
        fn label(&self) -> String {
            "GameOver".to_string()
        }
        fn update(&mut self, _: StateData<'_, ()>) -> Transition<(), ()> {
            if self.countdown > 0 {
                self.countdown -= 1;
                Transition::None
            } else {
                Transition::Quit
            }
        }
    }

    #[test]
    fn simulate_game() -> Result<()> {
        // Create the intro state
        let intro_countdown = 8;
        let intro_state = IntroState {
            countdown: intro_countdown,
        };

        // Start the state machine with the intro state
        let mut state_machine = StateMachine::new(intro_state);
        assert_eq!(state_machine.active_state_label(), None);
        state_machine
            .start(StateData::new(&mut ()))
            .context("Tried to start state machine with no states present!")?;

        // Play the intro
        for _ in 0..=intro_countdown {
            assert_eq!(
                state_machine.active_state_label(),
                Some("Intro".to_string())
            );
            state_machine.update(StateData::new(&mut ()));
            assert!(state_machine.is_running());
        }

        // Main Menu
        assert_eq!(
            state_machine.active_state_label(),
            Some("MainMenu".to_string())
        );
        state_machine.update(StateData::new(&mut ()));

        // Gameplay State
        assert_eq!(
            state_machine.active_state_label(),
            Some("Gameplay".to_string())
        );
        // Simulate some gameplay
        state_machine.update(StateData::new(&mut ()));
        // On the second pass we'll pause
        state_machine.update(StateData::new(&mut ()));

        // Pause Menu
        assert_eq!(
            state_machine.active_state_label(),
            Some("Pause".to_string())
        );
        // Unpause
        state_machine.update(StateData::new(&mut ()));

        // Back to the gameplay
        assert_eq!(
            state_machine.active_state_label(),
            Some("Gameplay".to_string())
        );
        // The game has ended
        state_machine.update(StateData::new(&mut ()));

        // Game Over
        assert_eq!(
            state_machine.active_state_label(),
            Some("GameOver".to_string())
        );
        // Exit the game
        state_machine.update(StateData::new(&mut ()));

        Ok(())
    }
}
