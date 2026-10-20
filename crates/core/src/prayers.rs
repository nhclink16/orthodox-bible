//! Built-in prayer texts (Orthodox fork), for a prayer rule's items (`rule::Kind::Prayers`).
//!
//! Only public-domain texts, checked against the page scans on archive.org: Isabel Hapgood's
//! Service Book of the Holy Orthodox-Catholic Apostolic Church (1906, `cu31924029363128`),
//! R. W. Blackmore's translation of the Russian Primer (1845, `doctrineofrussia00blac`) and N. F.
//! Robinson's Monasticism in the Orthodox Churches (1916, `monasticisminort0000nfro`). Wording,
//! capitals and rubrics ("(Thrice.)") are as printed; the home arrangements leave out the
//! priest's parts, and say "Through the prayers of our holy Fathers" where a priest would give the blessing. Modern
//! prayer books are under copyright; a rule names those ("From your prayer book") instead of
//! quoting them, or holds the person's own text.

pub struct Prayer {
    pub id: &'static str,
    pub title: &'static str,
    pub text: &'static str,
    /// Where the wording comes from.
    pub source: &'static str,
}

static PRAYERS: &[Prayer] = &[
    Prayer {
        id: "morning-prayers",
        title: "Morning Prayers",
        text: "Through the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.\nGlory to thee, our God; glory to thee.\nO heavenly King, the Comforter, Spirit of Truth, who art in all places and fillest all things; Treasury of good things and Giver of life: Come and take up thine abode in us, and cleanse us from every stain; and save our souls, O Good One.\nO Holy God, Holy Mighty, Holy Immortal One, have mercy upon us. (Thrice, and three reverences.)\nGlory to the Father, and to the Son, and to the Holy Spirit, now, and ever, and unto ages of ages. Amen.\nO all-holy Trinity, have mercy upon us. O Lord, wash away our sins. O Master, pardon our transgressions. O Holy One, visit and heal our infirmities, for thy Name's sake.\nLord, have mercy. (Thrice.)\nGlory to the Father, and to the Son, and to the Holy Spirit, now, and ever, and unto ages of ages. Amen.\nOur Father, who art in heaven, Hallowed be thy Name. Thy kingdom come. Thy will be done on earth, As it is in heaven. Give us this day our daily bread. And forgive us our trespasses, As we forgive those who trespass against us. And lead us not into temptation; But deliver us from the Evil One.\nThrough the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.\nLord, have mercy. (Thrice.)\nO come, let us worship God our King. O come, let us worship and fall down before Christ, our King and our God. O come, let us worship and fall down before the Very Christ, our King and our God. (Three reverences.)\n\nO Lord, I thank Thee with my whole heart that Thou hast raised me up from sleep safe and sound, that Thou hast scattered the darkness of night, and gladdened mine eyes with the light of this day. And now, O Lord, I believe in Thee with my whole heart; scatter, I pray Thee, the darkness of my ignorance, and of my passions, and lighten my mind and my will with Thy almighty grace. Grant me to do cheerfully and zealously every work which Thou hast appointed for me, and reverently to search out the wonders hidden in Thy holy law, ever calling with boldness upon Thy holy Name.\n\nO Christ, the true Light, which illumineth and sanctifieth every man who cometh into the world! Let the light of thy countenance be showed upon us, that in it we may behold the light ineffable; and guide our footsteps aright, to the keeping of thy commandments; through the intercessions of thine all-pure Mother, and of all the Saints. Amen.\n\nThrough the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.",
        source: "Arranged from Hapgood, Service Book (1906) (pp. 42, 147, 163) and Blackmore, The Russian Primer (1845) (p. 4)",
    },
    Prayer {
        id: "evening-prayers",
        title: "Evening Prayers",
        text: "Through the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.\nGlory to thee, our God; glory to thee.\nO heavenly King, the Comforter, Spirit of Truth, who art in all places and fillest all things; Treasury of good things and Giver of life: Come and take up thine abode in us, and cleanse us from every stain; and save our souls, O Good One.\nO Holy God, Holy Mighty, Holy Immortal One, have mercy upon us. (Thrice, and three reverences.)\nGlory to the Father, and to the Son, and to the Holy Spirit, now, and ever, and unto ages of ages. Amen.\nO all-holy Trinity, have mercy upon us. O Lord, wash away our sins. O Master, pardon our transgressions. O Holy One, visit and heal our infirmities, for thy Name's sake.\nLord, have mercy. (Thrice.)\nGlory to the Father, and to the Son, and to the Holy Spirit, now, and ever, and unto ages of ages. Amen.\nOur Father, who art in heaven, Hallowed be thy Name. Thy kingdom come. Thy will be done on earth, As it is in heaven. Give us this day our daily bread. And forgive us our trespasses, As we forgive those who trespass against us. And lead us not into temptation; But deliver us from the Evil One.\nThrough the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.\nLord, have mercy. (Thrice.)\nO come, let us worship God our King. O come, let us worship and fall down before Christ, our King and our God. O come, let us worship and fall down before the Very Christ, our King and our God. (Three reverences.)\n\nVouchsafe, O Lord, to keep us this night without sin. Blessed art thou, O Lord, the God of our fathers, and praised and glorified is thy Name forever. Amen.\nLet thy mercy, be upon us, O Lord, even as we have set our hope on thee. Blessed art thou, O Lord; teach me thy statutes. Blessed art thou, O Master; make me to understand thy commandments. Blessed art thou, O Holy One; enlighten me with thy precepts.\nThy mercy, O Lord, endureth forever: O despise not the works of thy hands. To thee belongeth worship, to thee belongeth praise, to thee belongeth glory, to the Father, and to the Son, and to the Holy Spirit, now, and ever, unto ages of ages. Amen.\n\nAnd grant unto us, O Master, as we lay us down to sleep, repose both of body and of soul, and preserve us from the gloomy slumber of sin, and from every dark and nocturnal sensuality. Calm thou the impulses of carnal desires: quench the fiery darts of the Evil One which are craftily directed against us. Assuage the rebellions of our flesh. Still our every earthly and material anxiety; and vouchsafe unto us, O God, a watchful mind, a chaste reason, a sober heart, sleep gentle and free from every vision of the Devil; and raise us up again at the hour of prayer, strengthened in thy precepts, and holding steadfastly within us the memory of thy commandments. Grant that all the night long we may sing praises unto thee, and that we may hymn, and bless, and glorify thine all-honourable and majestic Name, of the Father, and of the Son, and of the Holy Spirit, now, and ever, and unto ages of ages. Amen.\n\nAlmighty God, Who hast vouchsafed of Thy providence to bring me to this present hour of night; I give Thee hearty thanks for all the blessings that I have this day received at Thy hand, praying Thee to accept my contrition for all that I have done amiss. And now that I am going to my rest, keep me, I beseech Thee, under the shadow of Thy wings, and make me withal to remember that everlasting rest which Thou hast prepared for them that love Thee; among whom place me also, O Lord.\n\nThe Father is my hope; the Son is my refuge; the Holy Spirit is my protector. O Holy Trinity, glory to thee.\n\nThrough the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.",
        source: "Arranged from Hapgood, Service Book (1906) (pp. 147, 159–163) and Blackmore, The Russian Primer (1845) (p. 4)",
    },
    Prayer {
        id: "trisagion",
        title: "The Trisagion Prayers",
        text: "Through the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.\nGlory to thee, our God; glory to thee.\nO heavenly King, the Comforter, Spirit of Truth, who art in all places and fillest all things; Treasury of good things and Giver of life: Come and take up thine abode in us, and cleanse us from every stain; and save our souls, O Good One.\nO Holy God, Holy Mighty, Holy Immortal One, have mercy upon us. (Thrice, and three reverences.)\nGlory to the Father, and to the Son, and to the Holy Spirit, now, and ever, and unto ages of ages. Amen.\nO all-holy Trinity, have mercy upon us. O Lord, wash away our sins. O Master, pardon our transgressions. O Holy One, visit and heal our infirmities, for thy Name's sake.\nLord, have mercy. (Thrice.)\nGlory to the Father, and to the Son, and to the Holy Spirit, now, and ever, and unto ages of ages. Amen.\nOur Father, who art in heaven, Hallowed be thy Name. Thy kingdom come. Thy will be done on earth, As it is in heaven. Give us this day our daily bread. And forgive us our trespasses, As we forgive those who trespass against us. And lead us not into temptation; But deliver us from the Evil One.\nThrough the prayers of our holy Fathers, O Lord Jesus Christ our God, have mercy upon us. Amen.\nLord, have mercy. (Thrice.)\nO come, let us worship God our King. O come, let us worship and fall down before Christ, our King and our God. O come, let us worship and fall down before the Very Christ, our King and our God. (Three reverences.)",
        source: "Hapgood, Service Book (1906), pp. 147, 163; arranged for prayer at home",
    },
    Prayer {
        id: "before-meals",
        title: "The Lord's Prayer",
        text: "Our Father, who art in heaven, Hallowed be thy Name. Thy kingdom come. Thy will be done on earth, As it is in heaven. Give us this day our daily bread. And forgive us our trespasses, As we forgive those who trespass against us. And lead us not into temptation; But deliver us from the Evil One.",
        source: "Hapgood, Service Book (1906), p. 147",
    },
    Prayer {
        id: "meal-blessing",
        title: "Before Meals",
        text: "The eyes of all wait upon Thee, O Lord, and Thou givest them their meat in due season: Thou openest Thy hand, and fillest all things living with good.",
        source: "Blackmore, The Russian Primer (1845), p. 4",
    },
    Prayer {
        id: "after-meals",
        title: "After Meals",
        text: "I thank Thee, O Christ my God, that Thou hast fed me with Thy earthly good things: let it not be with these only, I beseech Thee, but bring me also to Thy heavenly kingdom.",
        source: "Blackmore, The Russian Primer (1845), p. 4",
    },
    Prayer {
        id: "jesus-prayer",
        title: "The Jesus Prayer",
        text: "Lord Jesus Christ, Son of God, have mercy upon me a sinner.",
        source: "N. F. Robinson, Monasticism in the Orthodox Churches (1916), p. 160",
    },
    Prayer {
        id: "st-ephrem",
        title: "Prayer of St Ephrem",
        text: "O Lord and Master of my life, grant not unto me a spirit of slothfulness, of discouragement, of lust of power, of vain babbling. (Reverence.)\nBut vouchsafe unto thy servant the spirit of continence, of meekness, of patience, and of love. (Reverence.)\nYea, O Lord and King, grant that I may perceive my own transgressions, and judge not my brother. For blessed art thou unto ages of ages. Amen. (Reverence. Then twelve lesser reverences, saying at each: O God, cleanse thou me, a sinner. Then the whole Prayer is repeated; with one great reverence in conclusion.)",
        source: "Hapgood, Service Book (1906), p. 41",
    },
    Prayer {
        id: "on-rising",
        title: "On Rising from Sleep",
        text: "O Lord, I thank Thee with my whole heart that Thou hast raised me up from sleep safe and sound, that Thou hast scattered the darkness of night, and gladdened mine eyes with the light of this day. And now, O Lord, I believe in Thee with my whole heart; scatter, I pray Thee, the darkness of my ignorance, and of my passions, and lighten my mind and my will with Thy almighty grace. Grant me to do cheerfully and zealously every work which Thou hast appointed for me, and reverently to search out the wonders hidden in Thy holy law, ever calling with boldness upon Thy holy Name.",
        source: "Blackmore, The Russian Primer (1845), p. 4",
    },
    Prayer {
        id: "christ-true-light",
        title: "O Christ, the True Light",
        text: "O Christ, the true Light, which illumineth and sanctifieth every man who cometh into the world! Let the light of thy countenance be showed upon us, that in it we may behold the light ineffable; and guide our footsteps aright, to the keeping of thy commandments; through the intercessions of thine all-pure Mother, and of all the Saints. Amen.",
        source: "Hapgood, Service Book (1906), p. 42 (the First Hour)",
    },
    Prayer {
        id: "on-going-to-rest",
        title: "On Going to Rest",
        text: "Almighty God, Who hast vouchsafed of Thy providence to bring me to this present hour of night; I give Thee hearty thanks for all the blessings that I have this day received at Thy hand, praying Thee to accept my contrition for all that I have done amiss. And now that I am going to my rest, keep me, I beseech Thee, under the shadow of Thy wings, and make me withal to remember that everlasting rest which Thou hast prepared for them that love Thee; among whom place me also, O Lord.",
        source: "Blackmore, The Russian Primer (1845), p. 4",
    },
];

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
        for id in ["morning-prayers", "evening-prayers", "trisagion", "st-ephrem", "jesus-prayer"] {
            let p = super::get(id).unwrap();
            assert!(!p.text.contains("Priest") && !p.text.contains("Reader."), "{id}");
        }
        assert!(super::get("evening-prayers").unwrap().text.contains("as we lay us down to sleep"));
        assert!(super::get("nonexistent").is_none());
    }
}
