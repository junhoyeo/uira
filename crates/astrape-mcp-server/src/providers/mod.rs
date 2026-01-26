pub mod gemini;
pub mod openai;

pub use gemini::GeminiProvider;
pub use openai::OpenAIProvider;

pub trait Provider {
    fn query(
        &self,
        prompt: &str,
        model: &str,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send;
}
