//! Container layout: rows and columns of fixed and flexible slots.
//!
//! Sizes are in logical px (`Size::Px`, multiplied by the UI scale factor at
//! solve time) or weights over the leftover space (`Size::Flex`). Containers
//! nest arbitrarily; `solve` walks the tree and emits a rect per leaf.

use spark_render::Viewport;

#[derive(Clone, Copy, Debug)]
pub enum Dir {
    Row,
    Col,
}

#[derive(Clone, Copy, Debug)]
pub enum Size {
    /// Fixed size in logical pixels (scaled by the UI scale factor).
    Px(f32),
    /// Weighted share of whatever space the fixed slots leave over.
    Flex(f32),
}

pub struct Node<L> {
    size: Size,
    kind: Kind<L>,
}

enum Kind<L> {
    Leaf(Option<L>),
    Split(Split<L>),
}

struct Split<L> {
    dir: Dir,
    gap: f32,
    pad: f32,
    children: Vec<Node<L>>,
}

impl<L: Copy> Node<L> {
    pub fn leaf(size: Size, id: L) -> Self {
        Self {
            size,
            kind: Kind::Leaf(Some(id)),
        }
    }

    /// An empty slot that takes up space but emits nothing.
    pub fn spacer(size: Size) -> Self {
        Self {
            size,
            kind: Kind::Leaf(None),
        }
    }

    pub fn row(size: Size) -> Self {
        Self::split(Dir::Row, size)
    }

    pub fn col(size: Size) -> Self {
        Self::split(Dir::Col, size)
    }

    fn split(dir: Dir, size: Size) -> Self {
        Self {
            size,
            kind: Kind::Split(Split {
                dir,
                gap: 0.0,
                pad: 0.0,
                children: Vec::new(),
            }),
        }
    }

    /// Logical px between children (containers only).
    pub fn gap(mut self, gap: f32) -> Self {
        if let Kind::Split(s) = &mut self.kind {
            s.gap = gap;
        }
        self
    }

    /// Logical px inset on all four sides (containers only).
    pub fn pad(mut self, pad: f32) -> Self {
        if let Kind::Split(s) = &mut self.kind {
            s.pad = pad;
        }
        self
    }

    pub fn child(mut self, child: Node<L>) -> Self {
        if let Kind::Split(s) = &mut self.kind {
            s.children.push(child);
        }
        self
    }

    /// Walk the tree, emitting one `(id, rect)` per leaf. `rect` is in
    /// physical pixels; `scale` converts `Size::Px` and gap/pad values.
    pub fn solve(&self, rect: Viewport, scale: f32, out: &mut Vec<(L, Viewport)>) {
        match &self.kind {
            Kind::Leaf(Some(id)) => out.push((*id, rect)),
            Kind::Leaf(None) => {}
            Kind::Split(s) => {
                let pad = s.pad * scale;
                let inner = Viewport {
                    x: rect.x + pad,
                    y: rect.y + pad,
                    w: (rect.w - 2.0 * pad).max(0.0),
                    h: (rect.h - 2.0 * pad).max(0.0),
                };
                let gap = s.gap * scale;
                let gaps = gap * s.children.len().saturating_sub(1) as f32;
                let main = match s.dir {
                    Dir::Row => inner.w,
                    Dir::Col => inner.h,
                };
                let mut fixed = 0.0;
                let mut flex = 0.0;
                for c in &s.children {
                    match c.size {
                        Size::Px(p) => fixed += p * scale,
                        Size::Flex(f) => flex += f,
                    }
                }
                let leftover = (main - fixed - gaps).max(0.0);
                let mut cursor = match s.dir {
                    Dir::Row => inner.x,
                    Dir::Col => inner.y,
                };
                for c in &s.children {
                    let len = match c.size {
                        Size::Px(p) => p * scale,
                        Size::Flex(f) if flex > 0.0 => leftover * f / flex,
                        Size::Flex(_) => 0.0,
                    };
                    let r = match s.dir {
                        Dir::Row => Viewport {
                            x: cursor,
                            y: inner.y,
                            w: len,
                            h: inner.h,
                        },
                        Dir::Col => Viewport {
                            x: inner.x,
                            y: cursor,
                            w: inner.w,
                            h: len,
                        },
                    };
                    c.solve(r, scale, out);
                    cursor += len + gap;
                }
            }
        }
    }
}
