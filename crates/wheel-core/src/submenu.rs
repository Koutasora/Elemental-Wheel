//! Submenu arc state: which arc is open for a hit, and which item a hit selects.

use crate::config::{Config, Item, MAX_SUBMENU_ITEMS};
use crate::geometry::{Hit, SubArc, clamp_outer_sectors};

/// The arc that stays or becomes open when the pointer is over `hit`.
/// An outer sector with a submenu opens its arc, a tile keeps its parent's arc open, anything
/// else closes it. Only sectors of the active outer ring (`< outer_sectors`) can open an arc.
pub fn arc_for_hit(hit: Hit, config: &Config) -> Option<SubArc> {
    let parent = match hit {
        Hit::Outer(i) | Hit::Sub(i, _) => i,
        Hit::Inner(_) | Hit::Center => return None,
    };
    if parent >= clamp_outer_sectors(config.options.outer_sectors) {
        return None;
    }
    let count = config
        .outer_item(usize::from(parent))?
        .submenu
        .len()
        .min(MAX_SUBMENU_ITEMS);
    // `count` is at most MAX_SUBMENU_ITEMS (5), so it fits in u8.
    (count > 0).then_some(SubArc {
        parent,
        count: count as u8,
    })
}

/// The item a hit selects; `None` for the center and for positions without an item.
pub fn item_for_hit(hit: Hit, config: &Config) -> Option<&Item> {
    match hit {
        Hit::Outer(i) => config.outer_item(usize::from(i)),
        Hit::Inner(i) => config.inner_item(usize::from(i)),
        Hit::Sub(p, k) => config
            .outer_item(usize::from(p))?
            .submenu
            .get(usize::from(k)),
        Hit::Center => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Default config with a submenu of `count` items on outer sector `parent`.
    fn config_with(parent: usize, count: usize, outer_sectors: u8) -> Config {
        let mut c = Config::default();
        c.options.outer_sectors = outer_sectors;
        c.outer[parent].submenu = (0..count)
            .map(|k| Item {
                name: format!("s{k}"),
                ..Item::default()
            })
            .collect();
        c
    }

    #[test]
    fn outer_with_submenu_opens_arc() {
        for count in 1..=5usize {
            let c = config_with(2, count, 12);
            assert_eq!(
                arc_for_hit(Hit::Outer(2), &c),
                Some(SubArc {
                    parent: 2,
                    count: count as u8
                })
            );
        }
    }

    #[test]
    fn arc_stays_on_tile_and_closes_elsewhere() {
        let c = config_with(2, 3, 12);
        let open = Some(SubArc {
            parent: 2,
            count: 3,
        });
        assert_eq!(arc_for_hit(Hit::Sub(2, 0), &c), open);
        assert_eq!(arc_for_hit(Hit::Sub(2, 2), &c), open);
        assert_eq!(arc_for_hit(Hit::Outer(3), &c), None);
        assert_eq!(arc_for_hit(Hit::Inner(2), &c), None);
        assert_eq!(arc_for_hit(Hit::Center, &c), None);
        // A tile of a parent without a submenu cannot keep an arc open.
        assert_eq!(arc_for_hit(Hit::Sub(5, 0), &c), None);
    }

    #[test]
    fn sector_beyond_outer_sectors_has_no_arc() {
        let c = config_with(8, 2, 6);
        assert_eq!(arc_for_hit(Hit::Outer(8), &c), None);
        assert_eq!(arc_for_hit(Hit::Sub(8, 0), &c), None);
        let c = config_with(5, 2, 6);
        assert!(arc_for_hit(Hit::Outer(5), &c).is_some());
    }

    #[test]
    fn hit_maps_to_item() {
        let c = config_with(4, 3, 12);
        assert_eq!(item_for_hit(Hit::Center, &c), None);
        assert_eq!(item_for_hit(Hit::Outer(4), &c), c.outer_item(4));
        assert_eq!(item_for_hit(Hit::Inner(7), &c), c.inner_item(7));
        assert_eq!(item_for_hit(Hit::Sub(4, 1), &c).unwrap().name, "s1");
        assert_eq!(item_for_hit(Hit::Sub(4, 3), &c), None);
        assert_eq!(item_for_hit(Hit::Sub(5, 0), &c), None);
        assert_eq!(item_for_hit(Hit::Outer(200), &c), None);
        assert_eq!(item_for_hit(Hit::Inner(200), &c), None);
    }

    #[test]
    fn empty_submenu_has_no_arc() {
        let c = config_with(3, 0, 12);
        assert_eq!(arc_for_hit(Hit::Outer(3), &c), None);
        assert_eq!(arc_for_hit(Hit::Outer(200), &c), None);
    }

    #[test]
    fn arc_count_is_capped() {
        // Validation rejects more than 5 items; the cap still protects hit testing.
        let c = config_with(1, 7, 12);
        assert_eq!(
            arc_for_hit(Hit::Outer(1), &c),
            Some(SubArc {
                parent: 1,
                count: MAX_SUBMENU_ITEMS as u8
            })
        );
    }

    #[test]
    fn out_of_range_sector_count_is_clamped() {
        // 0 is clamped to 4 sectors, 255 to 12.
        let c = config_with(3, 2, 0);
        assert!(arc_for_hit(Hit::Outer(3), &c).is_some());
        let c = config_with(4, 2, 0);
        assert_eq!(arc_for_hit(Hit::Outer(4), &c), None);
        let c = config_with(11, 2, 255);
        assert!(arc_for_hit(Hit::Outer(11), &c).is_some());
    }
}
