extern crate rand;

mod grid;
mod city;
mod agent;
use self::grid::{HexGrid, Position};
use self::city::{Parcel};

fn main() {
    let grid = HexGrid::<Parcel>::new(10, 10);
    let pos = Position(2,2);
    println!("{:?}", grid.adjacent(pos));
    println!("Hello, world!");
}
