use crate::mortality::{Mortality, MortalityChange};
use crate::{AnalyzerEvent, AnalyzerState, round::Round, time::GameTime};
use dod::{RoundState, Team, UserMessage};
use std::time::Duration;

#[derive(Debug, Default)]
pub enum ClanMatchDetection {
    #[default]
    WaitingForReset,
    WaitingForNormal {
        reset_time: GameTime,
    },
    MatchIsLive,
}

pub fn use_clan_match_detection_updates(
    max_normal_duration_from_reset: Duration,
    state: &mut AnalyzerState,
    event: &AnalyzerEvent,
) {
    // WaveTime carries mp_clan_respawntime, a CVAR that is only set to a
    // non-zero value in clan-match server configs. Seeing it with duration > 0
    // at any point — including mid-demo after joining a live match — is a
    // definitive clan-match signal that doesn't depend on the Reset→Start
    // sequence being present in the recording.
    if let AnalyzerEvent::UserMessage(UserMessage::WaveTime(wave_time)) = event {
        if wave_time.0 > Duration::ZERO {
            state.clan_match_detected = true;
        }
    }

    match (&state.clan_match_detection, event) {
        // ClanTimer fires when the countdown to a clan match begins. Observing
        // it before the match is live (WaitingForReset or WaitingForNormal) is
        // definitive proof this is a clan match, even if the Reset→Start
        // sequence is never completed in this recording. The state machine is
        // left unchanged so it can still transition normally when Reset arrives.
        (
            ClanMatchDetection::WaitingForReset | ClanMatchDetection::WaitingForNormal { .. },
            AnalyzerEvent::UserMessage(UserMessage::ClanTimer(_)),
        ) => {
            state.clan_match_detected = true;
        }

        // Assume the first RoundState with a reset is the match going live
        (
            ClanMatchDetection::WaitingForReset,
            AnalyzerEvent::UserMessage(UserMessage::RoundState(RoundState::Reset)),
        ) => {
            state.clan_match_detection = ClanMatchDetection::WaitingForNormal {
                reset_time: state.current_time.clone(),
            };
        }

        // Players and teams are scoreless after a reset; we infer the match is live
        (
            ClanMatchDetection::WaitingForNormal { reset_time },
            AnalyzerEvent::UserMessage(UserMessage::RoundState(RoundState::Start)),
        ) if state
            .players
            .iter()
            .all(|player| matches!(player.stats, (0, _, _)))
            && state.team_scores.get_team_score(Team::Allies) == 0
            && state.team_scores.get_team_score(Team::Axis) == 0 =>
        {
            state.rounds.clear();
            state.rounds.push(Round::Active {
                allies_kills: 0,
                axis_kills: 0,
                start_time: reset_time.clone(),
            });

            state.team_scores.reset();

            for player in state.players.iter_mut() {
                player.kill_streaks.clear();
                player.weapon_breakdown.clear();

                player.mortality.clear();
                player.mortality.push(MortalityChange::new(
                    state.current_time.clone(),
                    Mortality::Alive,
                ));
            }

            // The scoreboard-zeroing Reset→Start sequence is the definitive
            // signal that a clan match just went live.
            state.clan_match_detected = true;
            state.clan_match_detection = ClanMatchDetection::MatchIsLive;
        }

        // Too much time passed since the round reset. We infer that detector is stuck.
        (ClanMatchDetection::WaitingForNormal { reset_time }, _)
            if &state.current_time - reset_time > max_normal_duration_from_reset =>
        {
            state.clan_match_detection = ClanMatchDetection::WaitingForReset;
        }

        // Match is already live, but we observed a ClanTimer. We infer that match is restarting.
        (
            ClanMatchDetection::MatchIsLive,
            AnalyzerEvent::UserMessage(UserMessage::ClanTimer(_)),
        ) => state.clan_match_detection = ClanMatchDetection::WaitingForReset,

        _ => {}
    };
}
