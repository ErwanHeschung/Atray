//! Champ de texte d'une ligne : curseur, sélection, déplacements par mot. Indices en octets,
//! toujours sur une frontière de caractère.
use std::ops::Range;

#[derive(Default)]
pub struct Field {
    pub text: String,
    cursor: usize,
    /// Autre bout de la sélection (= cursor quand rien n'est sélectionné).
    anchor: usize,
}

impl Field {
    pub fn selection(&self) -> Range<usize> {
        self.cursor.min(self.anchor)..self.cursor.max(self.anchor)
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn selected(&self) -> &str {
        &self.text[self.selection()]
    }

    pub fn clear(&mut self) {
        *self = Field::default();
    }

    /// Remplace la sélection (frappe, collage).
    pub fn insert(&mut self, s: &str) {
        let sel = self.selection();
        self.text.replace_range(sel.clone(), s);
        self.cursor = sel.start + s.len();
        self.anchor = self.cursor;
    }

    /// Touches d'édition et de déplacement. false si la touche ne concerne pas le champ.
    pub fn key(&mut self, key: &str, ctrl: bool, shift: bool) -> bool {
        let to = match key {
            "left" if !shift && !ctrl && !self.selection().is_empty() => self.selection().start,
            "right" if !shift && !ctrl && !self.selection().is_empty() => self.selection().end,
            "left" => self.prev(self.cursor, ctrl),
            "right" => self.next(self.cursor, ctrl),
            "home" => 0,
            "end" => self.text.len(),
            "a" if ctrl => {
                (self.anchor, self.cursor) = (0, self.text.len());
                return true;
            }
            "backspace" | "delete" => {
                if self.selection().is_empty() {
                    let (c, back) = (self.cursor, key == "backspace");
                    self.anchor = if back { self.prev(c, ctrl) } else { self.next(c, ctrl) };
                }
                self.insert("");
                return true;
            }
            _ => return false,
        };
        self.cursor = to;
        if !shift {
            self.anchor = to;
        }
        true
    }

    /// Caractère (ou mot, avec Ctrl) précédent : saute les espaces, puis le mot.
    fn prev(&self, pos: usize, word: bool) -> usize {
        let mut chars = self.text[..pos].char_indices().rev().peekable();
        if !word {
            return chars.next().map_or(0, |(i, _)| i);
        }
        while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
        let mut start = chars.peek().map_or(0, |&(i, _)| i);
        while let Some((i, _)) = chars.next_if(|(_, c)| !c.is_whitespace()) {
            start = i;
        }
        start
    }

    /// Caractère suivant, ou début du mot suivant avec Ctrl (comme sous Windows).
    fn next(&self, pos: usize, word: bool) -> usize {
        let mut chars = self.text[pos..].char_indices().map(|(i, c)| (pos + i, c)).peekable();
        if !word {
            chars.next();
            return chars.peek().map_or(self.text.len(), |&(i, _)| i);
        }
        while chars.next_if(|(_, c)| !c.is_whitespace()).is_some() {}
        while chars.next_if(|(_, c)| c.is_whitespace()).is_some() {}
        chars.peek().map_or(self.text.len(), |&(i, _)| i)
    }
}

#[test]
fn editing() {
    let mut f = Field::default();
    f.insert("élan vital");
    assert!(f.key("left", true, false)); // début de « vital »
    assert_eq!(f.cursor(), 6);
    f.key("left", false, false);
    f.key("left", false, false);
    assert_eq!(f.cursor(), 4); // après « éla » : « é » fait 2 octets
    f.key("left", true, true); // sélectionne « éla »
    assert_eq!(f.selected(), "éla");
    f.insert("X");
    assert_eq!(f.text, "Xn vital");
    f.key("end", false, false);
    f.key("backspace", true, false);
    assert_eq!(f.text, "Xn ");
    f.key("home", false, false);
    f.key("delete", true, false); // jusqu'au début du mot suivant
    assert_eq!(f.text, "");
    f.insert("ab cd");
    f.key("a", true, false);
    assert_eq!(f.selected(), "ab cd");
    f.key("right", false, false); // replie la sélection à droite
    assert_eq!((f.cursor(), f.selected()), (5, ""));
    f.key("home", false, true);
    f.key("backspace", false, false);
    assert_eq!(f.text, "");
    assert!(!f.key("x", false, false));
}
