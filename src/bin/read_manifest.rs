use talaria::casg::Manifest;
use std::path::Path;

fn main() {
    let manifest_path = Path::new("/home/brett/.talaria/databases/versions/custom/cholera/20250918_040507/manifest.tal");
    
    match Manifest::load_file(manifest_path) {
        Ok(manifest) => {
            if let Some(data) = manifest.get_data() {
                println!("Version: {}", data.version);
                println!("Chunks count: {}", data.chunk_index.len());
                for (i, chunk) in data.chunk_index.iter().enumerate() {
                    println!("\nChunk {}:", i);
                    println!("  Hash: {}", chunk.hash);
                    println!("  Sequence count: {}", chunk.sequence_count);
                    println!("  Size: {}", chunk.size);
                    println!("  TaxonIds count: {}", chunk.taxon_ids.len());
                    if !chunk.taxon_ids.is_empty() {
                        println!("  First few taxon IDs: {:?}", &chunk.taxon_ids[..chunk.taxon_ids.len().min(5)]);
                    }
                }
            }
        }
        Err(e) => eprintln!("Error: {}", e),
    }
}
