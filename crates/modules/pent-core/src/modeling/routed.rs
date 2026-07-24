//! Model routing — selects best model from ai-server catalog.
use crate::modeling::model::AiModel;

pub fn select_model(complexity: &str, models: &[AiModel]) -> AiModel {
    let d = AiModel::default_model();
    match complexity {
        "high" | "critical" => models
            .iter()
            .find(|m| m.enabled && m.max_tokens >= 128000)
            .unwrap_or(&d)
            .clone(),
        "medium" => models.iter().find(|m| m.enabled).unwrap_or(&d).clone(),
        _ => models.first().unwrap_or(&d).clone(),
    }
}

pub fn estimate_cost(model: &AiModel, input_tokens: usize, output_tokens: usize) -> f64 {
    let price = match model.model_name.as_str() {
        "gpt-4o" => 2.50,
        "gpt-4o-mini" => 0.15,
        "gpt-4-turbo" => 10.0,
        "claude-3-opus" => 15.0,
        "claude-3-sonnet" => 3.0,
        "claude-3-haiku" => 0.25,
        _ => 1.0,
    };
    (input_tokens as f64 / 1e6) * price + (output_tokens as f64 / 1e6) * price * 4.0
}
