use super::{Mesh, MAX_TRIANGLES, MAX_VERTICES};

pub fn stl(data: &[u8]) -> Result<Mesh, String> {
    // Binary headers are arbitrary and may even begin with "solid".
    if data.len() >= 84 {
        let count = u32::from_le_bytes(data[80..84].try_into().unwrap()) as usize;
        if 84_u64 + count as u64 * 50 == data.len() as u64 {
            if count == 0 || count > MAX_TRIANGLES {
                return Err("STL must contain 1–100,000 triangles".into());
            }
            let mut vertices = Vec::with_capacity(count * 3);
            let mut triangles = Vec::with_capacity(count);
            for record in data[84..].as_chunks::<50>().0 {
                let base = vertices.len() as u32;
                for vertex in record[12..48].as_chunks::<12>().0 {
                    vertices.push(std::array::from_fn(|axis| {
                        f32::from_le_bytes(vertex[axis * 4..axis * 4 + 4].try_into().unwrap())
                            as f64
                    }));
                }
                triangles.push([base, base + 1, base + 2]);
            }
            let mesh = Mesh {
                vertices,
                triangles,
            };
            mesh.validate()?;
            return Ok(mesh);
        }
    }
    let text = std::str::from_utf8(data).map_err(|_| "Invalid or truncated binary STL")?;
    let mut lines = text.lines().map(str::trim).filter(|line| !line.is_empty());
    let mut vertices = Vec::new();
    let mut triangles = Vec::new();
    let mut solids = 0;
    while let Some(header) = lines.next() {
        if header.split_whitespace().next() != Some("solid") {
            return Err("Malformed ASCII STL: expected solid".into());
        }
        solids += 1;
        loop {
            let line = lines
                .next()
                .ok_or("Truncated ASCII STL: missing endsolid")?;
            let mut words = line.split_whitespace();
            if words.next() == Some("endsolid") {
                break;
            }
            words = line.split_whitespace();
            if words.next() != Some("facet") || words.next() != Some("normal") {
                return Err("Malformed ASCII STL facet".into());
            }
            for _ in 0..3 {
                parse_number(words.next())?;
            }
            if words.next().is_some() {
                return Err("Malformed ASCII STL normal".into());
            }
            if lines
                .next()
                .is_none_or(|line| !line.split_whitespace().eq(["outer", "loop"]))
            {
                return Err("Malformed ASCII STL: expected outer loop".into());
            }
            let base = vertices.len() as u32;
            if triangles.len() >= MAX_TRIANGLES {
                return Err("STL exceeds 100,000 triangles".into());
            }
            for _ in 0..3 {
                let mut words = lines
                    .next()
                    .ok_or("Truncated ASCII STL vertex")?
                    .split_whitespace();
                if words.next() != Some("vertex") {
                    return Err("Malformed ASCII STL vertex".into());
                }
                vertices.push([
                    parse_number(words.next())?,
                    parse_number(words.next())?,
                    parse_number(words.next())?,
                ]);
                if words.next().is_some() {
                    return Err("Malformed ASCII STL vertex".into());
                }
            }
            if lines.next() != Some("endloop") || lines.next() != Some("endfacet") {
                return Err("Malformed ASCII STL: expected endloop / endfacet".into());
            }
            triangles.push([base, base + 1, base + 2]);
        }
    }
    if solids == 0 {
        return Err("Empty STL file".into());
    }
    let mesh = Mesh {
        vertices,
        triangles,
    };
    mesh.validate()?;
    Ok(mesh)
}

fn parse_number(value: Option<&str>) -> Result<f64, String> {
    let number = value
        .ok_or("Missing mesh coordinate")?
        .parse::<f64>()
        .map_err(|_| "Invalid mesh coordinate")?;
    if !number.is_finite() || number.abs() > super::MAX_COORDINATE {
        return Err("Mesh coordinate is non-finite or out of range".into());
    }
    Ok(number)
}

/// Triangle and convex planar polygon OBJ faces; no material/texture dependencies.
pub fn obj(data: &[u8]) -> Result<Mesh, String> {
    let text = std::str::from_utf8(data).map_err(|_| "OBJ must be UTF-8/ASCII text")?;
    let mut vertices = Vec::<[f64; 3]>::new();
    let mut triangles = Vec::new();
    for (line_number, line) in text.lines().enumerate() {
        let mut words = line.split('#').next().unwrap_or("").split_whitespace();
        match words.next() {
            Some("v") => {
                if vertices.len() >= MAX_VERTICES {
                    return Err("OBJ exceeds 300,000 vertices".into());
                }
                let vertex = [
                    parse_number(words.next())?,
                    parse_number(words.next())?,
                    parse_number(words.next())?,
                ];
                if words.next().is_some() {
                    return Err(format!(
                        "OBJ line {}: homogeneous/colored vertices are not supported",
                        line_number + 1
                    ));
                }
                vertices.push(vertex);
            }
            Some("f") => {
                let mut face = Vec::new();
                for word in words {
                    if face.len() >= 256 {
                        return Err(
                            "OBJ face exceeds 256 vertices; triangulate it before import".into(),
                        );
                    }
                    let index = word
                        .split('/')
                        .next()
                        .unwrap_or("")
                        .parse::<i64>()
                        .map_err(|_| "Invalid OBJ face index")?;
                    let index = if index < 0 {
                        vertices.len() as i64 + index
                    } else {
                        index - 1
                    };
                    if index < 0 || index as usize >= vertices.len() {
                        return Err(format!(
                            "OBJ line {}: vertex index out of range",
                            line_number + 1
                        ));
                    }
                    face.push(index as u32);
                }
                if face.len() < 3 {
                    return Err("OBJ face needs at least three vertices".into());
                }
                if triangles.len() + face.len() - 2 > MAX_TRIANGLES {
                    return Err("OBJ exceeds 100,000 triangles".into());
                }
                check_convex_planar(&face, &vertices)?;
                for index in 1..face.len() - 1 {
                    triangles.push([face[0], face[index], face[index + 1]]);
                }
            }
            Some("curv" | "surf" | "cstype" | "p" | "l") => {
                return Err(
                    "OBJ must contain polygon surfaces; curves, points and lines are unsupported"
                        .into(),
                )
            }
            _ => {} // object/group/smoothing and texture/material references are metadata only.
        }
    }
    let mesh = Mesh {
        vertices,
        triangles,
    };
    mesh.validate()?;
    Ok(mesh)
}

fn check_convex_planar(face: &[u32], vertices: &[[f64; 3]]) -> Result<(), String> {
    if face.len() == 3 {
        return Ok(());
    }
    let points: Vec<_> = face.iter().map(|i| vertices[*i as usize]).collect();
    let n = super::normal([points[0], points[1], points[2]]);
    let length = n.iter().map(|v| v * v).sum::<f64>().sqrt();
    if length == 0.0 {
        return Err(
            "OBJ polygon has collinear leading vertices; triangulate it before import".into(),
        );
    }
    let n = n.map(|v| v / length);
    for i in 0..points.len() {
        let dot: f64 = (0..3)
            .map(|axis| (points[i][axis] - points[0][axis]) * n[axis])
            .sum();
        if dot.abs() > 1e-5 {
            return Err("Non-planar OBJ polygons must be triangulated before import".into());
        }
        let turn = super::normal([
            points[i],
            points[(i + 1) % points.len()],
            points[(i + 2) % points.len()],
        ]);
        if (0..3).map(|axis| turn[axis] * n[axis]).sum::<f64>() <= 0.0 {
            return Err(
                "Concave/degenerate OBJ polygons must be triangulated before import".into(),
            );
        }
        // Every vertex must lie on the same side of every edge. This also
        // rejects self-intersecting star polygons that have consistent turns.
        for point in &points {
            let side = super::normal([points[i], points[(i + 1) % points.len()], *point]);
            if (0..3).map(|axis| side[axis] * n[axis]).sum::<f64>() < -1e-9 {
                return Err(
                    "Concave/self-intersecting OBJ polygons must be triangulated before import"
                        .into(),
                );
            }
        }
    }
    Ok(())
}
