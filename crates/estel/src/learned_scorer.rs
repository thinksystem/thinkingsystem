// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2024 Jonathan Lee
// See top-level LICENSE for details.

use crate::chart_matcher::RenderSpec;
use crate::data_profiler::DimensionProfile;
use std::collections::HashMap;






#[derive(Debug, Clone)]
pub struct LearnedScorer {
    
    pub weights: Vec<f64>,
    
    pub bias: f64,
    
    
    pub feature_names: Vec<&'static str>,
}

impl LearnedScorer {
    
    
    pub fn from_weights(weights: Vec<f64>, bias: f64, feature_names: Vec<&'static str>) -> Self {
        Self {
            weights,
            bias,
            feature_names,
        }
    }

    
    
    
    pub fn default_head() -> Self {
        
        
        
        
        
        let weights = vec![
            0.25,  
            0.20,  
            0.20,  
            0.15,  
            0.10,  
            0.05,  
            0.05,  
            0.03,  
            0.02,  
            0.02,  
            0.01,  
            0.01,  
            -0.01, 
            0.05,  
        ];
        let bias = 0.0;
        Self {
            weights,
            bias,
            feature_names: FEATURE_NAMES.to_vec(),
        }
    }

    
    pub fn predict(&self, fv: &FeatureVector) -> f64 {
        let x = fv.to_vec();
        let mut s = self.bias;
        for (wi, xi) in self.weights.iter().zip(x.iter()) {
            s += wi * xi;
        }
        
        s.clamp(0.0, 1.0)
    }

    
    
    
    pub fn rerank_specs(
        &self,
        profiles: &[DimensionProfile],
        specs: Vec<RenderSpec>,
        symbolic_scores: Option<&HashMap<String, f64>>,
    ) -> Vec<(RenderSpec, f64)> {
        let stats = DatasetStats::from_profiles(profiles);
        let mut scored: Vec<(RenderSpec, f64)> = specs
            .into_iter()
            .map(|spec| {
                let sym = symbolic_scores
                    .and_then(|m| m.get(&spec.chart_name))
                    .copied()
                    .unwrap_or(0.0);
                let fv = FeatureVector::from_spec(&spec, &stats, sym);
                let score = self.predict(&fv);
                (spec, score)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored
    }
}


const FEATURE_NAMES: [&str; 14] = [
    "quality_score",
    "technical_feasibility",
    "semantic_appropriateness",
    "visual_effectiveness",
    "data_utilisation",
    "complexity_match",
    "dims_used_norm",
    "complete_flag",
    "avg_profile_quality",
    "temporal_present",
    "numeric_count_norm",
    "categorical_count_norm",
    "avg_cardinality_scaled",
    "symbolic_score",
];

#[derive(Debug, Clone, Default)]
pub struct DatasetStats {
    pub dims: usize,
    pub numeric_count: usize,
    pub categorical_count: usize,
    pub temporal_present: bool,
    pub avg_cardinality: f64,
    pub avg_profile_quality: f64,
}

impl DatasetStats {
    pub fn from_profiles(profiles: &[DimensionProfile]) -> Self {
        if profiles.is_empty() {
            return Self::default();
        }
        let dims = profiles.len();
        let mut numeric_count = 0;
        let mut categorical_count = 0;
        let mut temporal_present = false;
        let mut card_total = 0.0;
        let mut card_n = 0;
        let mut quality_total = 0.0;
        for p in profiles {
            match p.data_type {
                crate::api_graph::DataType::Numeric => numeric_count += 1,
                crate::api_graph::DataType::Categorical => categorical_count += 1,
                crate::api_graph::DataType::Temporal => temporal_present = true,
            }
            if let Some(c) = p.cardinality {
                card_total += c as f64;
                card_n += 1;
            }
            quality_total += p.quality_score;
        }
        let avg_cardinality = if card_n > 0 {
            card_total / card_n as f64
        } else {
            0.0
        };
        let avg_profile_quality = quality_total / dims as f64;
        Self {
            dims,
            numeric_count,
            categorical_count,
            temporal_present,
            avg_cardinality,
            avg_profile_quality,
        }
    }
}

#[derive(Debug, Clone)]
pub struct FeatureVector {
    pub quality_score: f64,
    pub technical_feasibility: f64,
    pub semantic_appropriateness: f64,
    pub visual_effectiveness: f64,
    pub data_utilisation: f64,
    pub complexity_match: f64,
    pub dims_used_norm: f64,
    pub complete_flag: f64,
    pub avg_profile_quality: f64,
    pub temporal_present: f64,
    pub numeric_count_norm: f64,
    pub categorical_count_norm: f64,
    pub avg_cardinality_scaled: f64,
    pub symbolic_score: f64,
}

impl FeatureVector {
    pub fn from_spec(spec: &RenderSpec, stats: &DatasetStats, symbolic_score: f64) -> Self {
        let (tech, sem, vis, util, comp) = if let Some(ds) = &spec.detailed_score {
            (
                ds.technical_feasibility,
                ds.semantic_appropriateness,
                ds.visual_effectiveness,
                ds.data_utilisation,
                ds.complexity_match,
            )
        } else {
            
            let q = spec.quality_score;
            (q, q, q, q, q)
        };
        let dims = stats.dims.max(1) as f64;
        let dims_used_norm = (spec.dimensions_used as f64 / dims).clamp(0.0, 1.0);
        let complete_flag = if spec.complete { 1.0 } else { 0.0 };
        let temporal_present = if stats.temporal_present { 1.0 } else { 0.0 };
        let numeric_count_norm = (stats.numeric_count as f64 / dims).clamp(0.0, 1.0);
        let categorical_count_norm = (stats.categorical_count as f64 / dims).clamp(0.0, 1.0);
        let avg_cardinality_scaled = (stats.avg_cardinality / 100.0).clamp(0.0, 1.0);
        Self {
            quality_score: spec.quality_score.clamp(0.0, 1.0),
            technical_feasibility: tech.clamp(0.0, 1.0),
            semantic_appropriateness: sem.clamp(0.0, 1.0),
            visual_effectiveness: vis.clamp(0.0, 1.0),
            data_utilisation: util.clamp(0.0, 1.0),
            complexity_match: comp.clamp(0.0, 1.0),
            dims_used_norm,
            complete_flag,
            avg_profile_quality: stats.avg_profile_quality.clamp(0.0, 1.0),
            temporal_present,
            numeric_count_norm,
            categorical_count_norm,
            avg_cardinality_scaled,
            symbolic_score: symbolic_score.clamp(0.0, 1.0),
        }
    }

    pub fn to_vec(&self) -> Vec<f64> {
        vec![
            self.quality_score,
            self.technical_feasibility,
            self.semantic_appropriateness,
            self.visual_effectiveness,
            self.data_utilisation,
            self.complexity_match,
            self.dims_used_norm,
            self.complete_flag,
            self.avg_profile_quality,
            self.temporal_present,
            self.numeric_count_norm,
            self.categorical_count_norm,
            self.avg_cardinality_scaled,
            self.symbolic_score,
        ]
    }
}
