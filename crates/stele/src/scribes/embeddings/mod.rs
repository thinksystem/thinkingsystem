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



use std::fmt::Debug;

pub trait EmbeddingAdapter: Send + Sync + Debug {
    fn embed(&self, text: &str) -> Vec<f32>;
}

#[derive(Debug, Default)]
pub struct LocalEmbeddingAdapter {
    dim: usize,
}

impl LocalEmbeddingAdapter {
    pub fn new(dim: usize) -> Self {
        Self { dim }
    }
}

impl EmbeddingAdapter for LocalEmbeddingAdapter {
    fn embed(&self, text: &str) -> Vec<f32> {
        
        let mut vec = vec![0.0; self.dim];
        for (i, byte) in text.bytes().enumerate() {
            let idx = i % self.dim;
            vec[idx] += (byte as f32) / 255.0;
        }
        let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        vec
    }
}


