//! Frame rates of the player

/// Frame rate of a `.anim`, with the whole-millisecond period the player actually runs at
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameRate {
    fps: u8,
    period_ms: u32,
}

impl FrameRate {
    /// Fastest rate the player runs at true speed
    pub const MAX_FPS: u8 = 100;

    /// Frame rate for `fps`, clamped to `1..=MAX_FPS`
    pub fn new(fps: u8) -> Self {
        let fps = fps.clamp(1, Self::MAX_FPS);
        Self {
            fps,
            period_ms: 1000 / u32::from(fps),
        }
    }

    /// Frame rate at which the given frame delays play as their own lengths, or the closest one
    ///
    /// The common tick of the delays is played exactly by any rate whose period divides
    /// it; the longest such period spends the fewest display frames. When no period does,
    /// the rate nearest the tick is used and each delay rounds to its period.
    pub fn from_delays(delays_ms: impl IntoIterator<Item = u32>) -> Self {
        let tick = delays_ms.into_iter().fold(0, gcd).max(1);

        // Rates over 90 share the 10 ms period; the last, 100, is the one that runs it exactly
        (1..=Self::MAX_FPS)
            .map(Self::new)
            .filter(|rate| tick.is_multiple_of(rate.period_ms))
            .max_by_key(|rate| rate.period_ms)
            .unwrap_or_else(|| Self::new(u8::try_from(1000 / tick).unwrap_or(u8::MAX)))
    }

    /// Display frames per second
    pub fn fps(self) -> u8 {
        self.fps
    }

    /// Milliseconds each display frame lasts
    pub fn period_ms(self) -> u32 {
        self.period_ms
    }

    /// Display frames a delay spans at this rate, never zero
    pub fn repeats(self, delay_ms: u32) -> u32 {
        (delay_ms.saturating_add(self.period_ms / 2) / self.period_ms).max(1)
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_frame_rate_follows_the_player_period() {
        let rate = |delays: &[u32]| {
            let r = FrameRate::from_delays(delays.iter().copied());
            (r.fps(), r.period_ms())
        };

        assert_eq!(rate(&[1000, 1000]), (1, 1000));
        assert_eq!(rate(&[2000, 4000]), (1, 1000));
        assert_eq!(rate(&[30, 60]), (33, 30));
        assert_eq!(rate(&[33, 67]), (100, 10));
        assert_eq!(rate(&[]), (100, 10));
        assert_eq!(rate(&[0, 0]), (100, 10));
    }

    #[test]
    fn the_frame_rate_prefers_a_period_that_divides_the_delays() {
        let rate = |delays: &[u32]| {
            let r = FrameRate::from_delays(delays.iter().copied());
            (r.fps(), r.period_ms())
        };

        // 1000 / 400 rounds to 2 fps, whose 500 ms period cannot play a 400 ms frame
        assert_eq!(rate(&[400, 1200]), (5, 200));
        assert_eq!(rate(&[300, 300]), (10, 100));
        assert_eq!(rate(&[700]), (10, 100));
        assert_eq!(rate(&[150, 450]), (20, 50));
        // 1000 / 22 is 45, so a 45 ms tick has an exact rate after all
        assert_eq!(rate(&[45, 90]), (22, 45));
        // Several rates share the 10 ms period; the exact one wins
        assert_eq!(rate(&[10, 20]), (100, 10));
    }

    #[test]
    fn every_delay_plays_for_its_own_length_when_a_period_divides_the_tick() {
        for delays in [[400, 1200], [300, 300], [700, 1400], [150, 450]] {
            let rate = FrameRate::from_delays(delays);

            for delay in delays {
                assert_eq!(
                    rate.repeats(delay) * rate.period_ms(),
                    delay,
                    "delays={delays:?} delay={delay}"
                );
            }
        }
    }

    #[test]
    fn repeats_round_to_the_player_period_and_never_drop_a_frame() {
        let at = |period_ms| FrameRate::new((1000 / period_ms) as u8);

        assert_eq!(at(1000).repeats(2000), 2);
        assert_eq!(at(1000).repeats(4000), 4);
        assert_eq!(at(500).repeats(400), 1);
        assert_eq!(at(500).repeats(1200), 2);
        assert_eq!(at(10).repeats(33), 3);
        assert_eq!(at(10).repeats(67), 7);
        assert_eq!(at(1000).repeats(1), 1);
        assert_eq!(at(1000).repeats(0), 1);
    }

    #[test]
    fn repeats_saturates_instead_of_overflowing_on_a_huge_delay() {
        assert_eq!(FrameRate::new(1).repeats(u32::MAX), u32::MAX / 1000);
    }

    #[test]
    fn new_clamps_to_the_player_range() {
        assert_eq!(FrameRate::new(0), FrameRate::new(1));
        assert_eq!(FrameRate::new(255), FrameRate::new(FrameRate::MAX_FPS));
        assert_eq!(FrameRate::new(30).period_ms(), 33);
    }
}
