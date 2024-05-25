use std::ops::{Add, Mul};

use crate::bp_model::WorldEntity;
use crate::pole_graph::CandPoleGraph;
use euclid::Vector2D;
use petgraph::prelude::*;
use plotters::coord::Shift;
use plotters::prelude::*;

use crate::position::*;

static POLE_COLOR: HSLColor = HSLColor(0.02, 0.95, 0.55);
static BLOCKER_COLOR: HSLColor = HSLColor(0.6, 0.95, 0.55);
static POWERABLE_COLOR: HSLColor = HSLColor(0.3, 0.95, 0.55);

pub struct Drawing {
    pub area: DrawingArea<BitMapBackend<'static>, Shift>,
    pub pixel_size: (u32, u32),
    pub shift: Vector2D<f64, MapSpace>,
    pub scale: f64,
}

impl Drawing {
    pub fn on_area(
        name: &'static str,
        area: TileBoundingBox,
        size: u32,
    ) -> Result<Drawing, Box<dyn std::error::Error>> {
        let pixel_size = (size, size);
        let shift = area.min.corner_map_pos().to_vector();
        let area_size = area.size();
        let scale = size as f64 / (area_size.width.max(area_size.height) as f64);
        let root = BitMapBackend::new(name, pixel_size).into_drawing_area();
        root.fill(&WHITE)?;
        Ok(Drawing {
            area: root,
            pixel_size,
            shift,
            scale,
        })
    }

    pub fn map_pos(&self, pt: MapPosition) -> (i32, i32) {
        pt.add(-self.shift)
            .mul(self.scale)
            .round()
            .to_i32()
            .to_tuple()
    }
    pub fn map_bbox(&self, bbox: BoundingBox) -> [(i32, i32); 2] {
        [self.map_pos(bbox.min), self.map_pos(bbox.max)]
    }

    pub fn draw_entity(&self, entity: &WorldEntity) -> Result<(), Box<dyn std::error::Error>> {
        let bounds = self.map_bbox(entity.world_bbox().round_out());

        let color = match entity.prototype.pole_data {
            Some(_) => POLE_COLOR,
            None => {
                if entity.uses_power() {
                    POWERABLE_COLOR
                } else {
                    BLOCKER_COLOR
                }
            }
        };
        self.area.draw(&Rectangle::new(bounds, color.filled()))?;
        self.area.draw(&Rectangle::new(
            bounds,
            BLACK.stroke_width((0.1 * self.scale).ceil() as u32),
        ))?;
        Ok(())
    }

    pub fn draw_all_entities<'a>(
        &self,
        entities: impl IntoIterator<Item = &'a WorldEntity>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for entity in entities {
            self.draw_entity(entity)?;
        }
        Ok(())
    }

    pub fn draw_pole_graph(
        &self,
        graph: &CandPoleGraph,
        color: impl Color,
        width: f64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for edge in graph.edge_references() {
            let (from, to) = graph.edge_endpoints(edge.id()).unwrap();
            let from = self.map_pos(graph[from].entity.position);
            let to = self.map_pos(graph[to].entity.position);
            self.area.draw(&PathElement::new(
                vec![from, to],
                color.stroke_width((width * self.scale).ceil() as u32),
            ))?;
        }
        Ok(())
    }

    pub fn show(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.area.present().map_err(Into::into)
    }
}
