// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License version 3
// as published by the Free Software Foundation.
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
// See the GNU Affero General Public License for more details.
// You should have received a copy of the GNU Affero General Public License
// along with this program. If not, see https://www.gnu.org/licenses/.

use std::cmp::Ordering;
use std::hash::{Hash, Hasher};
#[derive(Debug, Clone)]
pub struct GeometryToken {
    pub geometry_type: String,
    pub coordinates: Vec<Vec<Vec<f64>>>,
    pub geometries: Option<Vec<GeometryToken>>,
}
impl GeometryToken {
    pub fn new(geometry_type: String) -> Self {
        Self {
            geometry_type,
            coordinates: Vec::new(),
            geometries: None,
        }
    }
    pub fn with_coordinates(mut self, coords: Vec<Vec<Vec<f64>>>) -> Self {
        self.coordinates = coords;
        self
    }
    pub fn with_geometries(mut self, geometries: Vec<GeometryToken>) -> Self {
        self.geometries = Some(geometries);
        self
    }
    pub fn add_coordinate_set(&mut self, coords: Vec<Vec<f64>>) {
        self.coordinates.push(coords);
    }
    pub fn add_geometry(&mut self, geometry: GeometryToken) {
        self.geometries.get_or_insert_with(Vec::new).push(geometry);
    }
    pub fn validate(&self) -> Result<(), String> {
        match self.geometry_type.as_str() {
            "Point" => self.validate_point(),
            "LineString" | "Line" => self.validate_linestring(),
            "Polygon" => self.validate_polygon(),
            "MultiPoint" => self.validate_multi_point(),
            "MultiLineString" => self.validate_multi_linestring(),
            "MultiPolygon" => self.validate_multi_polygon(),
            "GeometryCollection" => self.validate_collection(),
            _ => Err(format!("Unsupported geometry type: {}", self.geometry_type)),
        }
    }
    fn validate_point(&self) -> Result<(), String> {
        let point = self
            .coordinates
            .first()
            .and_then(|v| v.first())
            .ok_or("Missing point coordinates")?;
        if point.len() != 2 {
            return Err("Point must have exactly 2 coordinates [longitude, latitude]".to_string());
        }
        Ok(())
    }
    fn validate_linestring(&self) -> Result<(), String> {
        let line = self
            .coordinates
            .first()
            .ok_or("Missing LineString coordinates")?;
        if line.len() < 2 {
            return Err("LineString must have at least 2 points".to_string());
        }
        for point in line {
            if point.len() != 2 {
                return Err("Each LineString point must have [longitude, latitude]".to_string());
            }
        }
        Ok(())
    }
    fn validate_polygon(&self) -> Result<(), String> {
        let rings = self
            .coordinates
            .first()
            .ok_or("Missing Polygon coordinates")?;
        if rings.is_empty() {
            return Err("Polygon must have at least one ring.".to_string());
        }
        for ring in rings {
            if ring.len() < 4 {
                return Err("Polygon ring must have at least 4 points".to_string());
            }
            if ring.first() != ring.last() {
                return Err(
                    "Polygon ring must be closed (first and last points must be identical)"
                        .to_string(),
                );
            }
        }
        Ok(())
    }
    fn validate_multi_point(&self) -> Result<(), String> {
        let points = self
            .coordinates
            .first()
            .ok_or("Missing MultiPoint coordinates")?;
        for point in points {
            if point.len() != 2 {
                return Err("Each MultiPoint coordinate must be [longitude, latitude]".to_string());
            }
        }
        Ok(())
    }
    fn validate_multi_linestring(&self) -> Result<(), String> {
        if self.coordinates.is_empty() {
            return Ok(());
        }
        for linestring in &self.coordinates {
            if linestring.len() < 2 {
                return Err(
                    "Each LineString in a MultiLineString must have at least 2 points".to_string(),
                );
            }
            for point in linestring {
                if point.len() != 2 {
                    return Err("Each point must have [longitude, latitude]".to_string());
                }
            }
        }
        Ok(())
    }
    fn validate_multi_polygon(&self) -> Result<(), String> {
        if self.coordinates.is_empty() {
            return Ok(());
        }
        for polygon in &self.coordinates {
            if polygon.is_empty() {
                return Err(
                    "Each polygon in a MultiPolygon must have at least one ring.".to_string(),
                );
            }
            for ring in polygon {
                if ring.len() < 4 {
                    return Err("Each Polygon ring must have at least 4 points".to_string());
                }
                if ring.first() != ring.last() {
                    return Err("Each Polygon ring must be closed".to_string());
                }
            }
        }
        Ok(())
    }
    fn validate_collection(&self) -> Result<(), String> {
        let geometries = self
            .geometries
            .as_ref()
            .ok_or("GeometryCollection must have a 'geometries' array")?;
        for geometry in geometries {
            geometry.validate()?;
        }
        Ok(())
    }
}
impl PartialEq for GeometryToken {
    fn eq(&self, other: &Self) -> bool {
        self.geometry_type == other.geometry_type
            && self.coordinates == other.coordinates
            && self.geometries == other.geometries
    }
}
impl Eq for GeometryToken {}
impl PartialOrd for GeometryToken {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.geometry_type.cmp(&other.geometry_type))
    }
}
impl Hash for GeometryToken {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.geometry_type.hash(state);
        for polygon in &self.coordinates {
            for ring in polygon {
                for coordinate in ring {
                    coordinate.to_bits().hash(state);
                }
            }
        }
        if let Some(geometries) = &self.geometries {
            geometries.hash(state);
        }
    }
}
impl std::fmt::Display for GeometryToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{{ type: \"{}\", ... }}", self.geometry_type)
    }
}
