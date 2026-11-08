//! Pastille qui glisse d'un élément actif au suivant (onglets, puces), et contenu qui arrive du
//! même côté. Le conteneur relève la position de ses enfants à chaque image ; son premier enfant
//! est la pastille (absolue), les vrais éléments suivent. Le conteneur ne doit pas avoir de padding.
use std::{cell::RefCell, rc::Rc, time::Instant};

use gpui::{Bounds, Div, Pixels, Stateful, Window, div, prelude::*, px};

use crate::theme::motion;

#[derive(Default)]
pub struct Slider {
    bounds: Rc<RefCell<Vec<Bounds<Pixels>>>>,
    /// Rectangle de départ (x, y, largeur, hauteur, relatifs au conteneur) et début du glissement.
    from: Option<([f32; 4], Instant)>,
}

impl Slider {
    fn target(&self, i: usize) -> Option<[f32; 4]> {
        let b = self.bounds.borrow();
        let origin = b.get(1)?.origin;
        let c = b.get(i + 1)?;
        Some([c.origin.x - origin.x, c.origin.y - origin.y, c.size.width, c.size.height].map(f32::from))
    }

    fn current(&self, i: usize) -> Option<[f32; 4]> {
        let to = self.target(i)?;
        Some(match self.from {
            Some((from, at)) => {
                let t = motion::eased(at, motion::PILL);
                [0, 1, 2, 3].map(|k| motion::lerp(from[k], to[k], t))
            }
            None => to,
        })
    }

    /// À appeler avant de changer l'élément actif (`old`) : la pastille partira de là où elle est.
    pub fn start(&mut self, old: usize) {
        self.from = self.current(old).map(|rect| (rect, Instant::now()));
    }

    pub fn reset(&mut self) {
        self.from = None;
    }

    pub fn is_sliding(&self) -> bool {
        self.from.is_some()
    }

    /// Dans `render` : rectangle de la pastille tant qu'elle glisse (et redemande une image).
    pub fn sliding(&mut self, window: &Window, active: usize) -> Option<[f32; 4]> {
        let (_, at) = self.from?;
        if motion::progress(window, at, motion::PILL) >= 1. {
            self.from = None;
            return None;
        }
        self.current(active)
    }

    /// Conteneur des éléments, avec la pastille en premier enfant (stylée par `pill`).
    pub fn container(&self, rect: Option<[f32; 4]>, pill: impl FnOnce(Div) -> Div) -> Div {
        let bounds = self.bounds.clone();
        div().relative().on_children_prepainted(move |b, _, _| *bounds.borrow_mut() = b).child(match rect {
            Some([x, y, w, h]) => pill(div().absolute().left(px(x)).top(px(y)).w(px(w)).h(px(h))),
            None => div().absolute().size_0(),
        })
    }
}

/// Changement de page ou de catégorie : sens (+1 vers la droite, -1 vers la gauche) et instant.
#[derive(Clone, Copy)]
pub struct Change(Instant, f32);

impl Default for Change {
    fn default() -> Self {
        Change(Instant::now() - motion::TAB * 4, 1.)
    }
}

impl Change {
    pub fn new(old: usize, new: usize) -> Self {
        Change(Instant::now(), if new > old { 1. } else { -1. })
    }

    /// Dans `render` : redemande des images jusqu'à l'arrivée du dernier des `count` éléments.
    pub fn animate(&self, window: &Window, count: usize) {
        motion::progress(window, self.0, motion::TAB + motion::TAB_STAGGER * count as u32);
    }

    /// Élément n : arrive du côté où va la pastille, en fondu, un peu après le précédent.
    pub fn arriving(&self, n: usize, el: Stateful<Div>) -> Stateful<Div> {
        let t = motion::eased(self.0 + motion::TAB_STAGGER * n as u32, motion::TAB);
        el.opacity(t).relative().left(px(self.1 * motion::TAB_SHIFT * (1. - t)))
    }
}
