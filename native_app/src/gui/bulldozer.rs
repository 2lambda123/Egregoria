use super::Tool;
use crate::input::{MouseButton, MouseInfo};
use crate::rendering::immediate::ImmediateDraw;
use crate::uiworld::UiWorld;
use common::Z_TOOL;
use egregoria::Egregoria;
use geom::Color;
use map_model::{Map, ProjectKind};

pub fn bulldozer(goria: &Egregoria, uiworld: &mut UiWorld) {
    let tool: &Tool = &*uiworld.read::<Tool>();
    let mouseinfo: &MouseInfo = &*uiworld.read::<MouseInfo>();
    let map: &Map = &*goria.read::<Map>();
    let draw: &mut ImmediateDraw = &mut *uiworld.write::<ImmediateDraw>();
    let mut commands = uiworld.commands();

    if !matches!(*tool, Tool::Bulldozer) {
        return;
    }

    let cur_proj = map.project(mouseinfo.unprojected);

    draw.circle(cur_proj.pos, 2.0).color(Color::RED).z(Z_TOOL);

    if mouseinfo.just_pressed.contains(&MouseButton::Left) {
        let mut potentially_empty = Vec::new();
        log::info!("bulldozer {:?}", cur_proj);
        match cur_proj.kind {
            ProjectKind::Inter(id) => {
                potentially_empty.extend(map.intersections()[id].neighbors(map.roads()));
                commands.map_remove_intersection(id)
            }
            ProjectKind::Road(id) => {
                let r = &map.roads()[id];

                potentially_empty.push(r.src);
                potentially_empty.push(r.dst);

                commands.map_remove_road(id);
            }
            ProjectKind::Building(id) => {
                commands.map_remove_building(id);
            }
            ProjectKind::Ground | ProjectKind::Lot(_) => {}
        }

        for id in potentially_empty {
            if map.intersections()[id].roads.is_empty() {
                commands.map_remove_intersection(id);
            }
        }
    }
}
