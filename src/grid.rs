use std::collections::HashSet;

pub trait Cell: Clone + PartialEq {}
impl<T> Cell for T where T: Clone + PartialEq {}

#[derive(Hash, Clone, Copy, PartialEq, Eq, Debug)]
pub struct Position(pub isize, pub isize);

const ODD_ADJACENT_POSITIONS: [(isize, isize); 6] = [
  (-1,  0), // upper left
  (-1,  1), // upper right
  ( 0, -1), // left
  ( 0,  1), // right
  ( 1,  0), // bottom left
  ( 1,  1)  // bottom right
];

const EVEN_ADJACENT_POSITIONS: [(isize, isize); 6] = [
  (-1, -1), // upper left
  (-1,  0), // upper right
  ( 0, -1), // left
  ( 0,  1), // right
  ( 1, -1), // bottom left
  ( 1,  0)  // bottom right
];

pub struct HexGrid<C: Cell> {
    rows: usize,
    cols: usize,
    grid: Vec<Vec<Option<C>>>
}

impl<C: Cell> HexGrid<C> {
    pub fn new(rows: usize, cols: usize) -> HexGrid<C> {
        // Initialize grid
        let mut grid = Vec::new();
        for _ in 0..rows {
            let row = (0..cols).map(|_| None).collect();
            grid.push(row);
        }
        HexGrid {
            rows: rows,
            cols: cols,
            grid: grid
        }
    }

    // Positions adjacent to specified position
    pub fn adjacent(&self, pos: Position) -> Vec<Position> {
        let shifts = if pos.0 % 2 == 0 {EVEN_ADJACENT_POSITIONS} else {ODD_ADJACENT_POSITIONS};
        shifts.iter()
            // Shift positions
            .map(|s| (pos.0 + s.0, pos.1 + s.1))

            // Check w/in grid bounds
            .filter(|p| p.0 >= 0 && p.0 < (self.rows as isize) && p.1 >= 0 && p.1 < (self.cols as isize))
            .map(|p| Position(p.0, p.1)).collect()
    }

    // Positions within a radius of the specified position
    pub fn radius(&self, pos: Position, r: usize) -> Vec<Position> {
        let mut neighbs = HashSet::new();
        let mut next = vec![pos];
        for _ in 0..r {
            let adj: Vec<Position> = next.iter().map(|&p| self.adjacent(p)).flatten().collect();
            neighbs.extend(adj.to_vec());
            next = adj;
        }
        return neighbs.into_iter().collect();
    }

    // Iterate over cells
    pub fn cells(&self) {
        self.grid.iter().map(|row| row).flatten();
    }

    // 2D euclidean distance
    pub fn distance(&self, a: Position, b: Position) -> f64 {
        return (((a.0 - b.0).pow(2) + (a.1 - b.1).pow(2)) as f64).sqrt()
    }
}