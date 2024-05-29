use std::ops::{Add, Mul};

use euclid::{vec2, Vector2D};
use petgraph::prelude::*;
use plotters::coord::Shift;
use plotters::prelude::*;

use crate::bp_model::{BpModel, WorldEntity};
use crate::pole_graph::WithPosition;
use crate::position::*;

static POLE_COLOR: HSLColor = HSLColor(0.02, 0.95, 0.4);
static BLOCKER_COLOR: RGBColor = RGBColor(0, (0.38 * 255.0) as u8, (0.57 * 255.0) as u8);
static POWERABLE_COLOR: HSLColor = HSLColor(0.3, 0.8, 0.35);
static BACKGROUND_COLOR: RGBColor = RGBColor(80, 80, 90);
static POLE_GRAPH_COLOR: RGBColor = RGBColor(20, 212, 255);

pub struct Drawing<'a> {
    pub area: DrawingArea<BitMapBackend<'a>, Shift>,
    // dimensions: (u32, u32),
    tile_shift: Vector2D<f64, MapSpace>,
    scale: i32,
    padding: i32,
}

impl <'a> Drawing<'a> {
    pub fn on_area(
        name: &'a impl AsRef<std::path::Path>,
        area: TileBoundingBox,
        pixels_per_tile: i32,
        padding: i32,
    ) -> Result<Drawing<'a>, Box<dyn std::error::Error>> {
        let tile_shift = area.min.corner_map_pos().to_vector();
        let size = (area.size() * pixels_per_tile).to_vector() + vec2(padding, padding) * 2;
        let dim = size.to_u32().to_tuple();
        let root = BitMapBackend::<'a,_>::new(name, dim).into_drawing_area();
        root.fill(&BACKGROUND_COLOR)?;

        Ok(Drawing {
            area: root,
            tile_shift,
            scale: pixels_per_tile,
            padding,
        })
    }

    pub fn map_pos(&self, pt: MapPosition) -> (i32, i32) {
        pt.add(-self.tile_shift)
            .mul(self.scale as f64)
            .round()
            .to_i32()
            .add(vec2(self.padding, self.padding))
            .to_tuple()
    }
    pub fn map_bbox(&self, bbox: BoundingBox) -> [(i32, i32); 2] {
        [self.map_pos(bbox.min), self.map_pos(bbox.max)]
    }

    pub fn draw_entity(&self, entity: &WorldEntity) -> Result<(), Box<dyn std::error::Error>> {
        let bounds = self.map_bbox(entity.world_bbox().round_out());

        let color = match entity.prototype.pole_data {
            Some(_) => POLE_COLOR.to_rgba(),
            None => {
                if entity.uses_power() {
                    POWERABLE_COLOR.to_rgba()
                } else {
                    BLOCKER_COLOR.to_rgba()
                }
            }
        };
        self.area.draw(&Rectangle::new(bounds, color.filled()))?;
        self.area.draw(&Rectangle::new(
            bounds,
            BLACK.stroke_width((0.1 * self.scale as f64).ceil() as u32),
        ))?;
        Ok(())
    }

    pub fn draw_all_entities<'b>(
        &self,
        entities: impl IntoIterator<Item = &'b WorldEntity>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for entity in entities {
            self.draw_entity(entity)?;
        }
        Ok(())
    }

    pub fn draw_pole_graph<N: WithPosition, E>(
        &self,
        graph: &UnGraph<N, E>,
        width: f64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        for edge in graph.edge_references() {
            let (from, to) = graph.edge_endpoints(edge.id()).unwrap();
            let from = self.map_pos(graph[from].position());
            let to = self.map_pos(graph[to].position());
            self.area.draw(&PathElement::new(
                vec![from, to],
                POLE_GRAPH_COLOR.stroke_width((width * self.scale as f64).ceil() as u32),
            ))?;
        }
        Ok(())
    }

    pub fn draw_model(&self, model: &BpModel) -> Result<(), Box<dyn std::error::Error>> {
        self.draw_all_entities(model.all_entities().map(|e| &e.entity))?;
        self.draw_pole_graph(&model.get_current_pole_graph().0, 0.2)?;
        Ok(())
    }

    pub fn show(&self) -> Result<(), Box<dyn std::error::Error>> {
        self.area.present().map_err(Into::into)
    }
}
