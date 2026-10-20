//! Built-in prayer texts (Orthodox fork), for a prayer rule's items (`rule::Kind::Prayers`).
//!
//! Only public-domain texts: the 1611/1769 King James wording of the Lord's Prayer, Isabel
//! Hapgood's Service Book (1906) and other English translations published before 1929. Modern
//! prayer books are under copyright; a rule names those ("From your prayer book") instead of
//! quoting them, or holds the person's own text.

pub struct Prayer {
    pub id: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    /// Where the wording comes from.
    pub source: &'static str,
}

static PRAYERS: &[Prayer] = &[Prayer {
    id: "before-meals",
    title: "The Lord's Prayer",
    text: "Our Father, which art in heaven, hallowed be thy name. Thy kingdom come. Thy will be done \
           in earth, as it is in heaven. Give us this day our daily bread. And forgive us our \
           debts, as we forgive our debtors. And lead us not into temptation, but deliver us from \
           evil.",
    source: "Matthew 6:9–13, King James Version",
}];

/// A few lines for the examination of conscience before sleep (the app's own words).
pub const REFLECTION: &str = "Thank God for the gifts of this day. Go back over it in His presence: \
     where you fell short in thought, word and deed, and what you left undone. Ask His forgiveness, \
     and forgive anyone who wronged you. Then commit the night to Him.";

pub fn get(id: &str) -> Option<&'static Prayer> {
    PRAYERS.iter().find(|p| p.id == id)
}

pub fn all() -> &'static [Prayer] {
    PRAYERS
}

#[cfg(test)]
mod tests {
    #[test]
    fn ids_are_unique_and_found() {
        let ids: std::collections::HashSet<_> = super::all().iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), super::all().len());
        assert!(super::get("before-meals").unwrap().text.starts_with("Our Father"));
        assert!(super::get("nonexistent").is_none());
    }
}
