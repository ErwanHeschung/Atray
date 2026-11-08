//! Affiche ce que trouvent les sources : `cargo run -p atray-core --example scan`.
fn main() {
    let items = atray_core::sources::scan_all(&[]);
    for i in &items {
        println!("{:<28} {:<40} icon={}", i.id, i.name, i.icon.as_ref().is_some_and(|p| p.exists()));
    }
    println!("{} éléments", items.len());
}
