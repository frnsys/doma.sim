mod grid;
use grid::{HexGrid, Position};

#[derive(PartialEq, Clone)]
struct Cell {}

fn main() {
    let grid = HexGrid::<Cell>::new(10, 10);
    let pos = Position(2,2);
    println!("{:?}", grid.adjacent(pos));
    println!("Hello, world!");
}
