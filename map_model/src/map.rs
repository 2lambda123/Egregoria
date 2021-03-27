use crate::procgen::Trees;
use crate::serializing::SerializedMap;
use crate::{
    Building, BuildingGen, BuildingID, BuildingKind, Intersection, IntersectionID, Lane, LaneID,
    LaneKind, LanePattern, Lot, LotID, LotKind, ParkingSpotID, ParkingSpots, ProjectKind, Road,
    RoadID, RoadSegmentKind, SpatialMap,
};
use geom::{Intersect, Shape, Vec2, AABB};
use geom::{Spline, OBB};
use ordered_float::OrderedFloat;
use serde::{Deserialize, Serialize};
use slotmap::DenseSlotMap;

pub type Roads = DenseSlotMap<RoadID, Road>;
pub type Lanes = DenseSlotMap<LaneID, Lane>;
pub type Intersections = DenseSlotMap<IntersectionID, Intersection>;
pub type Buildings = DenseSlotMap<BuildingID, Building>;
pub type Lots = DenseSlotMap<LotID, Lot>;

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct MapProject {
    pub pos: Vec2,
    pub kind: ProjectKind,
}

// can't derive Serialize because it would clone
#[derive(Deserialize)]
#[serde(from = "SerializedMap")]
pub struct Map {
    pub(crate) roads: Roads,
    pub(crate) lanes: Lanes,
    pub(crate) intersections: Intersections,
    pub(crate) buildings: Buildings,
    pub(crate) lots: Lots,
    pub(crate) spatial_map: SpatialMap,
    pub trees: Trees,
    pub parking: ParkingSpots,
    pub dirt_id: u32,
}

impl Default for Map {
    fn default() -> Self {
        Self::empty()
    }
}

impl Map {
    // Public API

    pub fn empty() -> Self {
        Self {
            roads: Roads::default(),
            lanes: Lanes::default(),
            intersections: Intersections::default(),
            parking: ParkingSpots::default(),
            buildings: Buildings::default(),
            lots: Lots::default(),
            trees: Trees::default(),
            dirt_id: 1,
            spatial_map: SpatialMap::default(),
        }
    }

    pub fn update_intersection(&mut self, id: IntersectionID, f: impl Fn(&mut Intersection)) {
        info!("update_intersection {:?}", id);
        let inter = unwrap_ret!(self.intersections.get_mut(id));
        f(inter);
        inter.update_traffic_control(&mut self.lanes, &self.roads);
        inter.update_turns(&self.lanes, &self.roads);
        self.dirt_id += 1;
    }

    pub fn remove_intersection(&mut self, src: IntersectionID) {
        info!("remove_intersection {:?}", src);
        self.dirt_id += 1;

        let inter = unwrap_ret!(self.intersections.remove(src));
        for road in inter.roads {
            self.remove_road(road);
        }

        self.spatial_map.remove(src);
    }

    pub fn remove_building(&mut self, b: BuildingID) -> Option<Building> {
        info!("remove_building {:?}", b);

        let b = self.buildings.remove(b);
        if let Some(b) = &b {
            self.spatial_map.remove(b.id)
        }
        self.dirt_id += b.is_some() as u32;
        b
    }

    pub fn make_connection(
        &mut self,
        from: MapProject,
        to: MapProject,
        interpoint: Option<Vec2>,
        pattern: &LanePattern,
    ) -> IntersectionID {
        let connection_segment = match interpoint {
            Some(x) => RoadSegmentKind::from_elbow(from.pos, to.pos, x),
            None => RoadSegmentKind::Straight,
        };

        let mut mk_inter = |proj: MapProject| match proj.kind {
            ProjectKind::Ground => self.add_intersection(proj.pos),
            ProjectKind::Inter(id) => id,
            ProjectKind::Road(id) => self.split_road(id, proj.pos),
            ProjectKind::Building(_) | ProjectKind::Lot(_) => unreachable!(),
        };

        let from = mk_inter(from);
        let to = mk_inter(to);

        self.connect(from, to, pattern, connection_segment);
        to
    }

    pub fn build_special_building(
        &mut self,
        road: RoadID,
        obb: &OBB,
        kind: BuildingKind,
        gen: BuildingGen,
    ) -> BuildingID {
        log::info!(
            "build special {:?} on {:?} with shape {:?}",
            kind,
            road,
            obb
        );
        self.dirt_id += 1;
        let to_clean: Vec<_> = self
            .spatial_map
            .query(obb)
            .filter_map(|obj| {
                if let ProjectKind::Lot(id) = obj {
                    if self.lots[id].shape.intersects(obb) {
                        return Some(id);
                    }
                }
                None
            })
            .collect();
        for id in to_clean {
            Self::cleanup_lot(
                &mut self.roads,
                &mut self.spatial_map,
                &self
                    .lots
                    .remove(id)
                    .expect("Lot was present in spatial map but not in Lots struct"),
            )
        }

        self.trees.remove_near_filter(obb.bbox(), |_| true);

        Building::make(
            &mut self.buildings,
            &mut self.spatial_map,
            &self.roads[road],
            *obb,
            kind,
            gen,
        )
    }

    pub fn build_house(&mut self, id: LotID) -> Option<BuildingID> {
        info!("build house on {:?}", id);
        self.dirt_id += 1;

        let roads = &mut self.roads;
        let buildings = &mut self.buildings;
        let spatial_map = &mut self.spatial_map;

        let lot = self.lots.remove(id)?;

        Self::cleanup_lot(roads, spatial_map, &lot);

        Some(Building::make(
            buildings,
            spatial_map,
            &roads[lot.parent],
            lot.shape,
            BuildingKind::House,
            BuildingGen::House,
        ))
    }

    pub fn remove_road(&mut self, road_id: RoadID) -> Option<Road> {
        info!("remove_road {:?}", road_id);

        self.dirt_id += 1;
        let road = self.roads.remove(road_id)?;

        self.spatial_map.remove(road_id);

        for (id, _) in road.lanes_iter() {
            self.lanes.remove(id);
            self.parking.remove_spots(id);
        }

        for &lot in &road.lots {
            self.lots.remove(lot);
            self.spatial_map.remove(lot);
        }

        if let Some(i) = self.intersections.get_mut(road.src) {
            i.remove_road(road_id);
        }
        if let Some(i) = self.intersections.get_mut(road.dst) {
            i.remove_road(road_id);
        }

        self.invalidate(road.src);
        self.invalidate(road.dst);
        Some(road)
    }

    pub fn set_lot_kind(&mut self, lot: LotID, kind: LotKind) {
        match self.lots.get_mut(lot) {
            Some(lot) => {
                lot.kind = kind;
                self.dirt_id += 1;
            }
            None => log::warn!("trying to set kind of non-existing lot {:?}", lot),
        }
    }

    pub fn clear(&mut self) {
        info!("clear");
        let before = std::mem::take(self);
        self.trees = before.trees;
    }

    // Private mutating

    pub(crate) fn add_intersection(&mut self, pos: Vec2) -> IntersectionID {
        Intersection::make(&mut self.intersections, &mut self.spatial_map, pos)
    }

    fn invalidate(&mut self, id: IntersectionID) {
        info!("invalidate {:?}", id);

        self.dirt_id += 1;
        let inter = unwrap_ret!(self.intersections.get_mut(id));

        if inter.roads.is_empty() {
            self.remove_intersection(id);
            return;
        }

        inter.update_interface_radius(&mut self.roads);

        for x in inter.roads.clone() {
            let road = unwrap_contlog!(
                self.roads.get(x),
                "intersection has unexisting road in list"
            );

            let other_end = unwrap_contlog!(
                self.intersections.get_mut(road.other_end(id)),
                "road is connected to unexisting intersection"
            );
            other_end.update_interface_radius(&mut self.roads);

            #[allow(clippy::indexing_slicing)] // borrowed before
            self.roads[x].update_lanes(&mut self.lanes, &mut self.parking);

            other_end.update_polygon(&self.roads);
        }

        #[allow(clippy::indexing_slicing)] // borrowed before
        let inter = &mut self.intersections[id];
        inter.update_traffic_control(&mut self.lanes, &self.roads);
        inter.update_turns(&self.lanes, &self.roads);
        inter.update_polygon(&self.roads);

        self.spatial_map.update(
            inter.id,
            inter
                .polygon
                .bbox()
                .union(AABB::centered(inter.pos, Vec2::splat(25.0))),
        );
    }

    pub(crate) fn split_road(&mut self, id: RoadID, pos: Vec2) -> IntersectionID {
        info!("split_road {:?} {:?}", id, pos);

        let r = self
            .remove_road(id)
            .expect("Trying to split unexisting road");
        let id = self.add_intersection(pos);

        let pat = r.pattern();
        match r.segment {
            RoadSegmentKind::Straight => {
                self.connect(r.src, id, &pat, RoadSegmentKind::Straight);
                self.connect(id, r.dst, &pat, RoadSegmentKind::Straight);
            }
            RoadSegmentKind::Curved((from_derivative, to_derivative)) => {
                let s = Spline {
                    from: r.points.first(),
                    to: r.points.last(),
                    from_derivative,
                    to_derivative,
                };
                let t_approx = s.project_t(pos, 1.0);

                let (s_from, s_to) = s.split_at(t_approx);

                self.connect(
                    r.src,
                    id,
                    &pat,
                    RoadSegmentKind::Curved((s_from.from_derivative, s_from.to_derivative)),
                );
                self.connect(
                    id,
                    r.dst,
                    &pat,
                    RoadSegmentKind::Curved((s_to.from_derivative, s_to.to_derivative)),
                );
            }
        }

        id
    }

    pub(crate) fn connect(
        &mut self,
        src_id: IntersectionID,
        dst_id: IntersectionID,
        pattern: &LanePattern,
        segment: RoadSegmentKind,
    ) -> Option<RoadID> {
        info!(
            "connect {:?} {:?} {:?} {:?}",
            src_id, dst_id, pattern, segment
        );
        self.dirt_id += 1;

        let src = self.intersections.get(src_id)?;
        let dst = self.intersections.get(dst_id)?;

        let r = Road::make(
            src,
            dst,
            segment,
            pattern,
            &mut self.roads,
            &mut self.lanes,
            &mut self.parking,
            &mut self.spatial_map,
        );
        let id = r.id;
        #[allow(clippy::indexing_slicing)]
        let r = &self.roads[id];

        self.intersections.get_mut(src_id)?.add_road(&self.roads, r);
        self.intersections.get_mut(dst_id)?.add_road(&self.roads, r);

        self.invalidate(src_id);
        self.invalidate(dst_id);

        Lot::remove_intersecting_lots(self, id);
        Lot::generate_along_road(self, id);

        let r = &self.roads[id];
        let d = r.width + 50.0;
        self.trees.remove_near_filter(r.bbox().expand(d), |tpos| {
            let rd = common::rand::rand3(tpos.x, tpos.y, 391.0) * 20.0;
            r.points.project(tpos).is_close(tpos, d - rd)
        });

        Some(id)
    }

    fn cleanup_lot(roads: &mut Roads, spatial_map: &mut SpatialMap, lot: &Lot) {
        let rlots = &mut roads[lot.parent].lots;
        rlots.remove(rlots.iter().position(|&x| x == lot.id).unwrap());
        spatial_map.remove(lot.id);
    }

    // Public helpers

    pub fn project(&self, pos: Vec2, tolerance: f32) -> MapProject {
        let mk_proj = move |kind| MapProject { pos, kind };

        let mut qroad = None;
        for obj in self.spatial_map.query_around(pos, tolerance) {
            match obj {
                ProjectKind::Inter(id) => {
                    let inter = self.intersections
                        .get(id)
                        .expect("Inter does not exist anymore, you seem to have forgotten to remove it from the spatial map.");

                    return MapProject {
                        pos: inter.pos,
                        kind: obj,
                    };
                }
                ProjectKind::Lot(id) => {
                    if self.lots
                        .get(id)
                        .expect("Lot does not exist anymore, you seem to have forgotten to remove it from the spatial map.")
                        .shape
                        .is_close(pos, tolerance) {
                        return mk_proj(ProjectKind::Lot(id));
                    }
                }
                ProjectKind::Road(id) => {
                    if qroad.is_some() { continue; }
                    let road = self.roads
                        .get(id)
                        .expect("Road does not exist anymore, you seem to have forgotten to remove it from the spatial map.");

                    let projected = road.points.project(pos);
                    if projected.is_close(pos, road.width * 0.5 + tolerance) {
                        qroad = Some((id, projected));
                    }
                },
                ProjectKind::Building(id) => {
                    return mk_proj(ProjectKind::Building(id));
                }
                ProjectKind::Ground => {}
            }
        }

        if let Some((id, pos)) = qroad {
            return MapProject {
                pos,
                kind: ProjectKind::Road(id),
            };
        }

        mk_proj(ProjectKind::Ground)
    }

    pub fn is_empty(&self) -> bool {
        self.roads.is_empty() && self.lanes.is_empty() && self.intersections.is_empty()
    }

    pub fn roads(&self) -> &Roads {
        &self.roads
    }
    pub fn lanes(&self) -> &Lanes {
        &self.lanes
    }
    pub fn intersections(&self) -> &Intersections {
        &self.intersections
    }
    pub fn buildings(&self) -> &Buildings {
        &self.buildings
    }
    pub fn lots(&self) -> &Lots {
        &self.lots
    }
    pub fn spatial_map(&self) -> &SpatialMap {
        &self.spatial_map
    }

    pub fn find_road(&self, src: IntersectionID, dst: IntersectionID) -> Option<RoadID> {
        for r in &self.intersections[src].roads {
            let road = &self.roads[*r];
            if road.src == src && road.dst == dst {
                return Some(road.id);
            }
        }
        None
    }

    pub fn nearest_lane(&self, p: Vec2, kind: LaneKind) -> Option<LaneID> {
        self.lanes
            .iter()
            .filter(|(_, x)| x.kind == kind)
            .min_by_key(|(_, lane)| OrderedFloat(lane.dist2_to(p)))
            .map(|(id, _)| id)
    }

    pub fn parking_to_drive(&self, spot: ParkingSpotID) -> Option<LaneID> {
        let spot = self.parking.get(spot)?;
        let park_lane = self
            .lanes
            .get(spot.parent)
            .expect("Parking spot has no parent >:(");
        let road = self
            .roads
            .get(park_lane.parent)
            .expect("Lane has no parent >:(");
        Some(
            road.outgoing_lanes_from(road.src)
                .iter()
                .rfind(|&&(_, kind)| kind == LaneKind::Driving)
                .map(|&(id, _)| id)
                .expect("Road with parking lane doesn't have driving lane >:("),
        )
    }
}
