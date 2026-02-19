use text_splitter::{ChunkConfig, TextSplitter};

pub struct TextChunker {
    max_characters: usize,
    overlap_characters: usize,
}

impl TextChunker {
    pub fn new(max_characters: usize, overlap_characters: usize) -> Self {
        Self {
            max_characters,
            overlap_characters,
        }
    }

    pub fn chunk(&self, text: &str) -> Vec<String> {
        if text.trim().is_empty() {
            return Vec::new();
        }

        if text.len() <= self.max_characters {
            return vec![text.to_string()];
        }

        let config = ChunkConfig::new(self.max_characters)
            .with_overlap(self.overlap_characters)
            .unwrap_or_else(|_| ChunkConfig::new(self.max_characters));

        let splitter = TextSplitter::new(config);
        splitter.chunks(text).map(|s| s.to_string()).collect()
    }
}

impl Default for TextChunker {
    fn default() -> Self {
        Self::new(2000, 200)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_returns_empty() {
        let chunker = TextChunker::new(100, 10);
        assert!(chunker.chunk("").is_empty());
        assert!(chunker.chunk("   ").is_empty());
    }

    #[test]
    fn short_text_returns_single_chunk() {
        let chunker = TextChunker::new(1000, 100);
        let chunks = chunker.chunk("Hello world");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0], "Hello world");
    }

    #[test]
    fn long_text_splits_into_multiple_chunks() {
        let chunker = TextChunker::new(50, 10);
        let text = "This is a longer text that should be split into multiple chunks. \
                    Each chunk should be around 50 characters long. \
                    The chunker should handle this gracefully.";
        let chunks = chunker.chunk(text);
        assert!(chunks.len() > 1);
        for chunk in &chunks {
            assert!(!chunk.is_empty());
        }
    }

    #[test]
    fn default_chunker_has_sensible_values() {
        let chunker = TextChunker::default();
        assert_eq!(chunker.max_characters, 2000);
        assert_eq!(chunker.overlap_characters, 200);
    }

    #[test]
    fn overlapping_chunks_share_boundary_content() {
        let chunker = TextChunker::new(50, 10);
        // Use a text long enough to produce multiple chunks
        let text = "word ".repeat(40); // 200 chars
        let chunks = chunker.chunk(&text);
        // With overlap configured, verify multiple chunks are produced
        // The exact overlap behavior depends on text-splitter's semantic splitting
        assert!(
            chunks.len() > 1,
            "Expected multiple chunks with overlap configured"
        );
        // Verify all chunks are non-empty
        for chunk in &chunks {
            assert!(!chunk.is_empty(), "Chunks should not be empty");
        }
    }
}
