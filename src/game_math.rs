//! Deterministic, integer-only 2D primitives for the Zenvora game-runtime foundation.
//! This module intentionally has no graphics backend or floating-point dependencies.

/// Integer world-space vector. Arithmetic is explicit and checked to avoid silent overflow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Vec2i {
    pub x: i64,
    pub y: i64,
}

impl Vec2i {
    pub const ZERO: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i64, y: i64) -> Self {
        Self { x, y }
    }

    pub fn checked_add(self, rhs: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_add(rhs.x)?,
            y: self.y.checked_add(rhs.y)?,
        })
    }

    pub fn checked_sub(self, rhs: Self) -> Option<Self> {
        Some(Self {
            x: self.x.checked_sub(rhs.x)?,
            y: self.y.checked_sub(rhs.y)?,
        })
    }
}

/// Axis-aligned rectangle with an origin and non-negative width/height.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RectI {
    pub origin: Vec2i,
    pub width: i64,
    pub height: i64,
}

impl RectI {
    pub const fn new(x: i64, y: i64, width: i64, height: i64) -> Self {
        Self { origin: Vec2i::new(x, y), width, height }
    }

    /// Returns false for negative dimensions or edges that overflow i64.
    pub fn is_valid(self) -> bool {
        self.width >= 0
            && self.height >= 0
            && self.origin.x.checked_add(self.width).is_some()
            && self.origin.y.checked_add(self.height).is_some()
    }

    /// Half-open AABB overlap: rectangles that only touch at an edge do not overlap.
    /// Invalid rectangles and zero-area rectangles never overlap.
    pub fn overlaps(self, other: Self) -> bool {
        if !self.is_valid() || !other.is_valid()
            || self.width == 0 || self.height == 0
            || other.width == 0 || other.height == 0
        {
            return false;
        }
        let Some(self_right) = self.origin.x.checked_add(self.width) else { return false; };
        let Some(self_bottom) = self.origin.y.checked_add(self.height) else { return false; };
        let Some(other_right) = other.origin.x.checked_add(other.width) else { return false; };
        let Some(other_bottom) = other.origin.y.checked_add(other.height) else { return false; };

        self.origin.x < other_right
            && other.origin.x < self_right
            && self.origin.y < other_bottom
            && other.origin.y < self_bottom
    }

    /// Translate the rectangle, returning None if the translated origin overflows.
    pub fn translated(self, delta: Vec2i) -> Option<Self> {
        let moved = Self {
            origin: self.origin.checked_add(delta)?,
            ..self
        };
        moved.is_valid().then_some(moved)
    }
}

#[cfg(test)]
mod tests {
    use super::{RectI, Vec2i};

    #[test]
    fn vector_add_and_subtract_are_checked() {
        assert_eq!(Vec2i::new(2, -4).checked_add(Vec2i::new(3, 9)), Some(Vec2i::new(5, 5)));
        assert_eq!(Vec2i::new(5, 5).checked_sub(Vec2i::new(3, 9)), Some(Vec2i::new(2, -4)));
        assert_eq!(Vec2i::new(i64::MAX, 0).checked_add(Vec2i::new(1, 0)), None);
        assert_eq!(Vec2i::new(i64::MIN, 0).checked_sub(Vec2i::new(1, 0)), None);
    }

    #[test]
    fn rectangles_overlap_for_intersection_and_containment() {
        assert!(RectI::new(0, 0, 10, 10).overlaps(RectI::new(5, 5, 10, 10)));
        assert!(RectI::new(0, 0, 10, 10).overlaps(RectI::new(2, 2, 1, 1)));
        assert!(RectI::new(-5, -5, 10, 10).overlaps(RectI::new(-1, -1, 2, 2)));
    }

    #[test]
    fn touching_edges_and_corners_do_not_overlap() {
        let base = RectI::new(0, 0, 10, 10);
        assert!(!base.overlaps(RectI::new(10, 0, 3, 3)));
        assert!(!base.overlaps(RectI::new(0, 10, 3, 3)));
        assert!(!base.overlaps(RectI::new(10, 10, 3, 3)));
    }

    #[test]
    fn zero_and_negative_dimensions_never_overlap() {
        let base = RectI::new(0, 0, 10, 10);
        assert!(!base.overlaps(RectI::new(1, 1, 0, 2)));
        assert!(!base.overlaps(RectI::new(1, 1, 2, 0)));
        assert!(!base.overlaps(RectI::new(1, 1, -2, 2)));
    }

    #[test]
    fn coordinate_overflow_is_rejected() {
        let invalid = RectI::new(i64::MAX - 1, 0, 4, 1);
        assert!(!invalid.is_valid());
        assert!(!invalid.overlaps(RectI::new(i64::MAX - 1, 0, 1, 1)));
        assert_eq!(RectI::new(i64::MAX, 0, 1, 1).translated(Vec2i::new(1, 0)), None);
    }

    #[test]
    fn translation_handles_negative_coordinates() {
        let rect = RectI::new(2, 3, 4, 5);
        assert_eq!(rect.translated(Vec2i::new(-7, -9)), Some(RectI::new(-5, -6, 4, 5)));
    }
}
/// Immutable input snapshot for one simulation tick. Backends should sample device
/// state once and pass this value into the deterministic simulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct InputSnapshot {
    pub left: bool,
    pub right: bool,
    pub up: bool,
    pub down: bool,
    pub action: bool,
}

/// Minimal integer-only headless world. Coordinates use one world unit per tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GridWorld {
    pub player: Vec2i,
    pub width: i64,
    pub height: i64,
    pub obstacles: Vec<RectI>,
    pub frame: u64,
}

impl GridWorld {
    pub fn new(player: Vec2i, width: i64, height: i64, obstacles: Vec<RectI>) -> Option<Self> {
        if width <= 0 || height <= 0 { return None; }
        let world = Self { player, width, height, obstacles, frame: 0 };
        if !world.is_walkable(player) { return None; }
        Some(world)
    }

    fn is_walkable(&self, position: Vec2i) -> bool {
        if position.x < 0 || position.y < 0 || position.x >= self.width || position.y >= self.height {
            return false;
        }
        let player_rect = RectI::new(position.x, position.y, 1, 1);
        !self.obstacles.iter().any(|obstacle| player_rect.overlaps(*obstacle))
    }

    /// Applies one input snapshot. Opposing directions cancel; diagonal movement
    /// is intentionally disabled so every tick advances at most one grid cell.
    /// A blocked move leaves the player in place but still advances the frame.
    pub fn step(&mut self, input: InputSnapshot) -> Result<(), &'static str> {
        let dx = (if input.right && !input.left { 1 } else { 0 })
            - (if input.left && !input.right { 1 } else { 0 });
        let dy = (if input.down && !input.up { 1 } else { 0 })
            - (if input.up && !input.down { 1 } else { 0 });
        let (dx, dy) = if dx != 0 { (dx, 0) } else { (0, dy) };
        let next = self.player.checked_add(Vec2i::new(dx, dy));
        if let Some(position) = next {
            if self.is_walkable(position) { self.player = position; }
        }
        self.frame = self.frame.checked_add(1).ok_or("frame counter overflow")?;
        Ok(())
    }
}


#[cfg(test)]
mod simulation_tests {
    use super::{GridWorld, InputSnapshot, RectI, Vec2i};

    #[test]
    fn input_moves_one_tile_and_blocks_obstacles_and_bounds() {
        let mut world = GridWorld::new(
            Vec2i::new(1, 1), 5, 4,
            vec![RectI::new(2, 1, 1, 1)],
        ).unwrap();

        world.step(InputSnapshot { right: true, ..InputSnapshot::default() }).unwrap();
        assert_eq!(world.player, Vec2i::new(1, 1), "obstacle blocks movement");
        world.step(InputSnapshot { down: true, ..InputSnapshot::default() }).unwrap();
        assert_eq!(world.player, Vec2i::new(1, 2));
        world.step(InputSnapshot { left: true, ..InputSnapshot::default() }).unwrap();
        assert_eq!(world.player, Vec2i::new(0, 2));
        world.step(InputSnapshot { left: true, ..InputSnapshot::default() }).unwrap();
        assert_eq!(world.player, Vec2i::new(0, 2), "world boundary blocks movement");
        assert_eq!(world.frame, 4);
    }

    #[test]
    fn opposing_inputs_cancel_and_diagonal_prefers_horizontal() {
        let mut world = GridWorld::new(Vec2i::new(1, 1), 5, 5, vec![]).unwrap();
        world.step(InputSnapshot { left: true, right: true, ..InputSnapshot::default() }).unwrap();
        assert_eq!(world.player, Vec2i::new(1, 1));
        world.step(InputSnapshot { right: true, down: true, ..InputSnapshot::default() }).unwrap();
        assert_eq!(world.player, Vec2i::new(2, 1));
    }

    #[test]
    fn invalid_world_or_blocked_spawn_is_rejected() {
        assert!(GridWorld::new(Vec2i::new(0, 0), 0, 4, vec![]).is_none());
        assert!(GridWorld::new(Vec2i::new(1, 1), 4, 4, vec![RectI::new(1, 1, 1, 1)]).is_none());
        assert!(GridWorld::new(Vec2i::new(-1, 0), 4, 4, vec![]).is_none());
    }

    #[test]
    fn same_input_sequence_produces_same_world_state() {
        let mut a = GridWorld::new(Vec2i::new(0, 0), 8, 8, vec![RectI::new(2, 0, 1, 1)]).unwrap();
        let mut b = a.clone();
        let inputs = [
            InputSnapshot { right: true, ..InputSnapshot::default() },
            InputSnapshot { down: true, ..InputSnapshot::default() },
            InputSnapshot { right: true, ..InputSnapshot::default() },
        ];
        for input in inputs {
            a.step(input).unwrap();
            b.step(input).unwrap();
        }
        assert_eq!(a, b);
    }
}
