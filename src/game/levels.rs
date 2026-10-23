//! Dragon level rules.

pub const MAX_DRAGON_LEVEL: i64 = 40;

pub fn level_from_xp(xp: i64) -> i64 {
    if xp <= 0 {
        return 1;
    }
    ((xp / 4) + 1).min(MAX_DRAGON_LEVEL)
}

pub fn raw_level(xp: i64) -> i64 {
    xp.div_euclid(4) + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_follow_experience() {
        assert_eq!(level_from_xp(0), 1);
        assert_eq!(level_from_xp(-5), 1);
        assert_eq!(level_from_xp(7), 2);
        assert_eq!(level_from_xp(10_000), MAX_DRAGON_LEVEL);
        assert_eq!(raw_level(8), 3);
    }
}
