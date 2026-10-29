use super::{
    error::InitError,
    prompt::{PromptRequest, PromptResponse},
};

pub(super) fn select_retained(
    title: &str,
    options: &[String],
    retained: &[String],
    mut answer: impl FnMut(&PromptRequest) -> Result<PromptResponse, InitError>,
) -> Result<Vec<String>, InitError> {
    if options.is_empty() {
        return Err(anyhow::anyhow!("no setup choices are available").into());
    }
    loop {
        let request =
            PromptRequest::multi_select(title, options, &[]).with_retained_selection(retained);
        let answer = answer(&request)?;
        let Some(indices) = answer.selected_indices() else {
            continue;
        };
        if indices.is_empty() {
            eprintln!("Select at least one option explicitly.");
            continue;
        }
        let mut selected = Vec::new();
        for &index in indices {
            if let Some(option) = options.get(index) {
                selected.push(option.clone());
            }
        }
        if !selected.is_empty() {
            return Ok(selected);
        }
    }
}

#[cfg(test)]
mod test_select {
    use super::{PromptResponse, select_retained};

    #[test]
    fn empty_answer_reprompts_even_for_a_single_candidate() {
        let mut answers = [
            PromptResponse::multi_select(&[]),
            PromptResponse::multi_select(&[0]),
        ]
        .into_iter();
        let mut prompts = 0;
        let selection = select_retained("Choose engine", &["sqlite".to_owned()], &[], |request| {
            assert!(request.automatic_response().is_none());
            prompts += 1;
            Ok(answers.next().unwrap())
        })
        .unwrap();
        assert_eq!(prompts, 2);
        assert_eq!(selection, ["sqlite"]);
    }
}
