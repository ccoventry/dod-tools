use crate::{AnalyzerEvent, AnalyzerState};
use dem::types::EngineMessage;
use std::{ops::Sub, time::Duration};

/// A moment in time when something happened in game.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct GameTime {
    /// Timestamp that represents the amount opf time relative to 0 (recording start).
    pub real_offset: Duration,

    /// Timestamp that represents the value shown in the `viewdemo` window.
    pub viewdemo_offset: Duration,

    /// The 1-based frame index at which this timestamp was recorded.
    /// Populated from `AnalyzerState::frame_index` each time a Frame event fires.
    /// Used by the Capture Studio to resolve wall-clock positions into patcher ticks.
    pub frame_index: usize,
}

impl Sub<GameTime> for GameTime {
    type Output = Duration;

    fn sub(self, rhs: GameTime) -> Self::Output {
        self.viewdemo_offset
            .checked_sub(rhs.viewdemo_offset)
            .unwrap_or(Duration::ZERO)
    }
}

impl<'a> Sub<&'a GameTime> for &GameTime {
    type Output = Duration;

    fn sub(self, rhs: &'a GameTime) -> Self::Output {
        self.viewdemo_offset
            .checked_sub(rhs.viewdemo_offset)
            .unwrap_or(Duration::ZERO)
    }
}

pub fn use_timing_updates(state: &mut AnalyzerState, event: &AnalyzerEvent) {
    if let AnalyzerEvent::EngineMessage(EngineMessage::SvcTime(svc_time)) = event
        && let Ok(offset) = Duration::try_from_secs_f32(svc_time.time)
    {
        state.current_time.viewdemo_offset = offset;
    } else if let AnalyzerEvent::Frame(frame) = event {
        state.frame_index += 1;
        state.current_time.frame_index = state.frame_index;
        if let Ok(offset) = Duration::try_from_secs_f32(frame.time) {
            state.current_time.real_offset = offset;
        }
    }
}
