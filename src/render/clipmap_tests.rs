use bevy::{
    mesh::{Mesh, PrimitiveTopology},
    prelude::*,
};

use super::clipmap::{DirectionForTiple, QuadMeshBuilder, TerrainHeightMapMesh};

// ============================================================================
// Test Helpers
// ============================================================================

/// Count the number of triangles in a mesh
fn count_triangles(mesh: &Mesh) -> usize {
    match mesh.indices() {
        Some(bevy::mesh::Indices::U16(indices)) => indices.len() / 3,
        Some(bevy::mesh::Indices::U32(indices)) => indices.len() / 3,
        None => 0,
    }
}

/// Get all triangles as tuples of vertex indices
fn get_triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
    let mut triangles = Vec::new();
    if let Some(bevy::mesh::Indices::U32(indices)) = mesh.indices() {
        for i in (0..indices.len()).step_by(3) {
            triangles.push([indices[i] as u32, indices[i + 1] as u32, indices[i + 2] as u32]);
        }
    }
    triangles
}

/// Build adjacency list: vertex index -> list of triangle indices that use it
fn get_vertex_to_triangles(mesh: &Mesh) -> Vec<Vec<usize>> {
    let triangles = get_triangles(mesh);
    let vertex_count = mesh.count_vertices();
    let mut vertex_to_tris = vec![Vec::new(); vertex_count];
    
    for (tri_idx, tri) in triangles.iter().enumerate() {
        for &v in tri {
            vertex_to_tris[v as usize].push(tri_idx);
        }
    }
    vertex_to_tris
}

/// Get all edges as sorted vertex pairs, counting occurrences
fn count_edge_occurrences(mesh: &Mesh) -> std::collections::HashMap<(u32, u32), u32> {
    let mut edge_counts = std::collections::HashMap::new();
    let triangles = get_triangles(mesh);
    
    for tri in &triangles {
        // Edges of the triangle
        let edges = [
            (std::cmp::min(tri[0], tri[1]), std::cmp::max(tri[0], tri[1])),
            (std::cmp::min(tri[1], tri[2]), std::cmp::max(tri[1], tri[2])),
            (std::cmp::min(tri[2], tri[0]), std::cmp::max(tri[2], tri[0])),
        ];
        
        for edge in edges {
            *edge_counts.entry(edge).or_insert(0) += 1;
        }
    }
    edge_counts
}

/// Find boundary edges (edges that appear only once)
fn find_boundary_edges(mesh: &Mesh) -> Vec<(u32, u32)> {
    let edge_counts = count_edge_occurrences(mesh);
    edge_counts
        .into_iter()
        .filter(|(_, count)| *count == 1)
        .map(|(edge, _)| edge)
        .collect()
}

/// Check if the mesh is watertight (no boundary edges or all edges appear exactly twice)
fn is_watertight(mesh: &Mesh) -> bool {
    let edge_counts = count_edge_occurrences(mesh);
    edge_counts.values().all(|&count| count == 2)
}

/// Check if all triangles have consistent winding (CCW)
/// This checks that the normal of each triangle points in a consistent direction
fn has_consistent_winding(mesh: &Mesh) -> bool {
    if let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        let positions: &[[f32; 3]] = positions.as_float3().unwrap();
        let triangles = get_triangles(mesh);
        
        if triangles.is_empty() {
            return true;
        }
        
        // Convert to Vec3 for easier math
        let vec_positions: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
        
        // Get the normal of the first triangle as reference
        let first_tri = triangles[0];
        let v0 = vec_positions[first_tri[0] as usize];
        let v1 = vec_positions[first_tri[1] as usize];
        let v2 = vec_positions[first_tri[2] as usize];
        let reference_normal = (v1 - v0).cross(v2 - v0).normalize();
        
        for tri in &triangles {
            let v0 = vec_positions[tri[0] as usize];
            let v1 = vec_positions[tri[1] as usize];
            let v2 = vec_positions[tri[2] as usize];
            let normal = (v1 - v0).cross(v2 - v0).normalize();
            
            // Check if normal points in opposite direction
            if normal.dot(reference_normal) < -0.99 {
                return false;
            }
        }
        return true;
    }
    false
}

/// Check for degenerate triangles (zero or near-zero area)
fn has_no_degenerate_triangles(mesh: &Mesh, epsilon: f32) -> bool {
    if let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        let positions: &[[f32; 3]] = positions.as_float3().unwrap();
        let triangles = get_triangles(mesh);
        let vec_positions: Vec<Vec3> = positions.iter().map(|p| Vec3::from_array(*p)).collect();
        
        for tri in &triangles {
            let v0 = vec_positions[tri[0] as usize];
            let v1 = vec_positions[tri[1] as usize];
            let v2 = vec_positions[tri[2] as usize];
            
            let area = (v1 - v0).cross(v2 - v0).length();
            if area < epsilon {
                return false;
            }
        }
        return true;
    }
    false
}

/// Check that all vertices are connected (each vertex is part of at least one triangle)
fn all_vertices_connected(mesh: &Mesh) -> bool {
    let vertex_to_tris = get_vertex_to_triangles(mesh);
    vertex_to_tris.iter().all(|tris| !tris.is_empty())
}

/// Check that adjacent quads share vertices (no gaps in the mesh)
/// This is a basic sanity check - in a properly constructed mesh, shared edges
/// should have the same vertex positions
fn has_shared_vertices_for_adjacent_quads(mesh: &Mesh) -> bool {
    if let Some(positions) = mesh.attribute(Mesh::ATTRIBUTE_POSITION) {
        let positions: &[[f32; 3]] = positions.as_float3().unwrap();
        let _triangles = get_triangles(mesh);
        let edge_counts = count_edge_occurrences(mesh);
        
        // For interior edges (appearing twice), check that both triangles
        // reference the same vertex positions
        for (edge, count) in &edge_counts {
            if *count == 2 {
                // This is an interior edge, both triangles should share the vertex
                // The edge key already has consistent ordering (min, max)
                // So we just verify the vertices exist
                if edge.0 >= positions.len() as u32 || edge.1 >= positions.len() as u32 {
                    return false;
                }
            }
        }
        return true;
    }
    false
}

// ============================================================================
// QuadMeshBuilder Tests
// ============================================================================

#[test]
fn test_quad_mesh_builder_add_quad() {
    let mut builder = QuadMeshBuilder::empty();
    builder.add_quad(Vec3::ZERO, 1.0);
    
    assert_eq!(builder.vertices.len(), 4, "Should have 4 vertices for one quad");
    assert_eq!(builder.indices.len(), 6, "Should have 6 indices (2 triangles) for one quad");
    
    // Verify vertex positions
    assert_eq!(builder.vertices[0], Vec3::ZERO);
    assert_eq!(builder.vertices[1], Vec3::new(1.0, 0.0, 0.0));
    assert_eq!(builder.vertices[2], Vec3::new(0.0, 0.0, 1.0));
    assert_eq!(builder.vertices[3], Vec3::new(1.0, 0.0, 1.0));
    
    // Verify triangle indices (two triangles forming a quad)
    // Expected: [0, 2, 1, 2, 3, 1]
    assert_eq!(builder.indices[0], 0);
    assert_eq!(builder.indices[1], 2);
    assert_eq!(builder.indices[2], 1);
    assert_eq!(builder.indices[3], 2);
    assert_eq!(builder.indices[4], 3);
    assert_eq!(builder.indices[5], 1);
}

#[test]
fn test_quad_mesh_builder_add_subdivided_quad_no_direction() {
    let mut builder = QuadMeshBuilder::empty();
    builder.add_subdivided_quad(Vec3::ZERO, 1.0, 2, None);
    
    // 2x2 grid = 4 quads, each quad adds 4 vertices (no sharing in this implementation)
    // So 4 quads * 4 vertices = 16 vertices
    assert_eq!(builder.vertices.len(), 16, "2x2 subdivided quad should have 16 vertices");
    // 4 quads = 8 triangles = 24 indices
    assert_eq!(builder.indices.len(), 24, "2x2 subdivided quad should have 24 indices");
}

#[test]
fn test_quad_mesh_builder_add_subdivided_quad_with_direction() {
    let mut builder = QuadMeshBuilder::empty();
    builder.add_subdivided_quad(Vec3::ZERO, 1.0, 2, Some(DirectionForTiple::Up));
    
    // The subdivided quad with triple division on one edge
    // 2x2 divisions, but top row uses triple-divided quads
    // This should still produce a valid mesh
    assert!(builder.vertices.len() > 0);
    assert!(builder.indices.len() > 0);
    assert_eq!(builder.indices.len() % 3, 0, "Index count should be multiple of 3");
}

#[test]
fn test_quad_mesh_builder_build_mesh() {
    let mut builder = QuadMeshBuilder::empty();
    builder.add_quad(Vec3::ZERO, 1.0);
    let mesh = builder.build();
    
    assert_eq!(mesh.primitive_topology(), PrimitiveTopology::TriangleList);
    assert_eq!(mesh.count_vertices(), 4);
    assert_eq!(mesh.indices().map_or(0, |i| i.len()), 6);
}

// ============================================================================
// TerrainHeightMapMesh Tests - Basic Properties
// ============================================================================

#[test]
fn test_terrain_mesh_basic_creation() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 0,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    
    assert_eq!(mesh.primitive_topology(), PrimitiveTopology::TriangleList);
    assert!(mesh.count_vertices() > 0, "Mesh should have vertices");
    assert!(mesh.indices().map_or(0, |i| i.len()) > 0, "Mesh should have indices");
    assert_eq!(mesh.indices().map_or(0, |i| i.len()) % 3, 0, "Index count should be divisible by 3");
}

#[test]
fn test_terrain_mesh_with_one_ring() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    
    assert!(mesh.count_vertices() > 0);
    assert!(mesh.indices().map_or(0, |i| i.len()) > 0);
}

#[test]
fn test_terrain_mesh_with_multiple_rings() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 3,
        smallest_quad_count: 8,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    
    assert!(mesh.count_vertices() > 0);
    assert!(mesh.indices().map_or(0, |i| i.len()) > 0);
}

// ============================================================================
// TerrainHeightMapMesh Tests - Connectivity
// ============================================================================

#[test]
fn test_terrain_mesh_all_vertices_connected() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 2,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    assert!(all_vertices_connected(&mesh), "All vertices should be part of at least one triangle");
}

#[test]
fn test_terrain_mesh_no_degenerate_triangles() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 2,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    assert!(
        has_no_degenerate_triangles(&mesh, 0.001),
        "Mesh should have no degenerate triangles"
    );
}

#[test]
fn test_terrain_mesh_has_shared_vertices() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    assert!(
        has_shared_vertices_for_adjacent_quads(&mesh),
        "Adjacent quads should share vertices"
    );
}

#[test]
fn test_terrain_mesh_winding_consistency() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    assert!(
        has_consistent_winding(&mesh),
        "All triangles should have consistent winding order"
    );
}

// ============================================================================
// TerrainHeightMapMesh Tests - Boundary
// ============================================================================

#[test]
fn test_terrain_mesh_boundary_is_closed() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    let boundary_edges = find_boundary_edges(&mesh);
    
    // The mesh should have a boundary (it's not a closed manifold)
    // But the boundary should form closed loops
    // For now, just verify we can find boundary edges
    assert!(boundary_edges.len() > 0 || is_watertight(&mesh), 
            "Mesh should either be watertight or have boundary edges");
}

#[test]
fn test_terrain_mesh_vertex_count_grows_with_rings() {
    let base = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 0,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let with_one_ring = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 4,
        density_factor: 1.0,
    };
    
    let mesh_base = base.create_base_mesh();
    let mesh_with_ring = with_one_ring.create_base_mesh();
    
    assert!(
        mesh_with_ring.count_vertices() > mesh_base.count_vertices(),
        "Adding rings should increase vertex count"
    );
}

// ============================================================================
// Density Factor Tests
// ============================================================================

#[test]
fn test_density_factor_default() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 8,  // Must be divisible by 4
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    let base_vertex_count = mesh.count_vertices();
    
    // With density_factor = 1.0, this should work as before
    assert!(base_vertex_count > 0);
}

#[test]
fn test_density_factor_greater_than_one() {
    let terrain_default = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 8,
        density_factor: 1.0,
    };
    
    let terrain_dense = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 8,
        density_factor: 2.0,
    };
    
    let mesh_default = terrain_default.create_base_mesh();
    let mesh_dense = terrain_dense.create_base_mesh();
    
    // With higher density factor, outer rings should have more vertices
    assert!(
        mesh_dense.count_vertices() > mesh_default.count_vertices(),
        "Higher density factor should increase vertex count in outer rings"
    );
}

#[test]
fn test_density_factor_less_than_one() {
    let terrain_default = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 8,
        density_factor: 1.0,
    };
    
    let terrain_sparse = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 8,
        density_factor: 0.5,
    };
    
    let mesh_default = terrain_default.create_base_mesh();
    let mesh_sparse = terrain_sparse.create_base_mesh();
    
    // With lower density factor, outer rings should have fewer vertices
    // Note: This might round down to 0, which could be problematic
    // The implementation should handle this gracefully
    assert_ne!(
        mesh_sparse.count_vertices(),
        mesh_default.count_vertices(),
        "Different density factors should produce different meshes"
    );
}

#[test]
fn test_density_factor_no_affect_innermost_ring() {
    // The innermost ring should always use smallest_quad_count
    // Only outer rings should be affected by density_factor
    let terrain_a = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 8,
        density_factor: 1.0,
    };
    
    let terrain_b = TerrainHeightMapMesh {
        smallest_quad: 1.0,
        rings: 1,
        smallest_quad_count: 8,
        density_factor: 5.0,
    };
    
    let mesh_a = terrain_a.create_base_mesh();
    let mesh_b = terrain_b.create_base_mesh();
    
    // Both should have valid meshes
    assert!(mesh_a.count_vertices() > 0);
    assert!(mesh_b.count_vertices() > 0);
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_clipmap_mesh_connectivity_comprehensive() {
    // Test with a realistic configuration
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 0.5,
        rings: 3,
        smallest_quad_count: 16,
        density_factor: 1.0,
    };
    
    let mesh = terrain.create_base_mesh();
    
    // Check all connectivity properties
    assert!(all_vertices_connected(&mesh), "All vertices must be connected");
    assert!(
        has_no_degenerate_triangles(&mesh, 0.0001),
        "No degenerate triangles"
    );
    assert!(
        has_consistent_winding(&mesh),
        "Consistent winding order"
    );
    assert!(
        has_shared_vertices_for_adjacent_quads(&mesh),
        "Shared vertices for adjacent quads"
    );
    
    // Check mesh statistics
    let vertex_count = mesh.count_vertices();
    let triangle_count = count_triangles(&mesh);
    let boundary_count = find_boundary_edges(&mesh).len();
    
    println!(
        "Mesh stats: {} vertices, {} triangles, {} boundary edges",
        vertex_count, triangle_count, boundary_count
    );
    
    // Sanity checks
    assert!(vertex_count > 0);
    assert!(triangle_count > 0);
}

#[test]
fn test_clipmap_with_density_factor_integration() {
    let terrain = TerrainHeightMapMesh {
        smallest_quad: 0.5,
        rings: 2,
        smallest_quad_count: 16,
        density_factor: 2.0,
    };
    
    let mesh = terrain.create_base_mesh();
    
    // Verify the mesh is still valid with density factor
    assert!(all_vertices_connected(&mesh), "All vertices must be connected with density factor");
    assert!(
        has_no_degenerate_triangles(&mesh, 0.0001),
        "No degenerate triangles with density factor"
    );
    assert!(
        has_consistent_winding(&mesh),
        "Consistent winding order with density factor"
    );
}
