//! 16-bit RGB565 palette mirroring the original Pong Wars colors.
//!
//! Source palette from <https://github.com/vnglst/pong-wars>:
//! ```text
//! arctic-powder         #F1F6F4
//! mystic-mint           #D9E8E3
//! forsythia             #FFC801
//! deep-saffron          #FF9932
//! nocturnal-expedition  #114C5A
//! oceanic-noir          #172B36
//! ```
//!
//! Day team:  fill = mystic-mint,           ball = nocturnal-expedition
//! Night team: fill = nocturnal-expedition, ball = mystic-mint
//!
//! Encoded as RGB565 (the native pixel format for `embedded-graphics`'
//! `Rgb565`) so the firmware can hand them straight to the framebuffer
//! without an extra conversion step on every cell.

/// 16-bit RGB565 color word (RRRRRGGG GGGBBBBB).
pub type Rgb565 = u16;

/// Convert 24-bit RGB to RGB565 in `const` context.
pub const fn rgb565(r: u8, g: u8, b: u8) -> Rgb565 {
    ((r as u16 & 0xF8) << 8) | ((g as u16 & 0xFC) << 3) | ((b as u16 & 0xF8) >> 3)
}

/// `#D9E8E3` — the day team's filled-cell color.
pub const COLOR_MYSTIC_MINT: Rgb565 = rgb565(0xD9, 0xE8, 0xE3);
/// `#114C5A` — the night team's filled-cell color and the day team's ball.
pub const COLOR_NOCTURNAL_EXPEDITION: Rgb565 = rgb565(0x11, 0x4C, 0x5A);

/// Identifier of one of the two teams playing the game.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Team {
    /// The bright "day" team — starts on the left half of the grid.
    Day = 0,
    /// The dark "night" team — starts on the right half of the grid.
    Night = 1,
}

impl Team {
    /// The opposing team.
    #[inline]
    pub const fn opposite(self) -> Team {
        match self {
            Team::Day => Team::Night,
            Team::Night => Team::Day,
        }
    }
}

/// Colors used to paint the simulation.  All fields are RGB565.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    /// Color of cells owned by the day team.
    pub day_cell: Rgb565,
    /// Color of cells owned by the night team.
    pub night_cell: Rgb565,
    /// Color of the day team's ball (mirrors `night_cell` by tradition).
    pub day_ball: Rgb565,
    /// Color of the night team's ball (mirrors `day_cell`).
    pub night_ball: Rgb565,
}

impl Palette {
    /// The default palette matching the original web version.
    pub const fn classic() -> Self {
        Self {
            day_cell: COLOR_MYSTIC_MINT,
            night_cell: COLOR_NOCTURNAL_EXPEDITION,
            day_ball: COLOR_NOCTURNAL_EXPEDITION,
            night_ball: COLOR_MYSTIC_MINT,
        }
    }

    /// The cell fill color for a given team.
    #[inline]
    pub const fn cell_color(&self, team: Team) -> Rgb565 {
        match team {
            Team::Day => self.day_cell,
            Team::Night => self.night_cell,
        }
    }

    /// The ball color for a given team.
    #[inline]
    pub const fn ball_color(&self, team: Team) -> Rgb565 {
        match team {
            Team::Day => self.day_ball,
            Team::Night => self.night_ball,
        }
    }
}

impl Default for Palette {
    fn default() -> Self {
        Self::classic()
    }
}
