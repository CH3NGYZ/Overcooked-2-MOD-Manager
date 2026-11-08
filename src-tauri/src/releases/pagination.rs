use std::future::Future;

pub(crate) const RELEASE_PAGE_SIZE: u32 = 50;

/// The raw response count controls pagination, before invalid releases or source
/// assets are filtered. Errors never turn an incomplete snapshot into success.
pub(crate) async fn collect_release_pages<T, E, F, Fut>(mut fetch: F) -> Result<Vec<T>, E>
where
    F: FnMut(u32) -> Fut,
    Fut: Future<Output = Result<(Vec<T>, usize), E>>,
{
    let mut all = Vec::new();
    let mut page = 1;
    loop {
        let (items, received) = fetch(page).await?;
        all.extend(items);
        if received < RELEASE_PAGE_SIZE as usize {
            return Ok(all);
        }
        page += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn full_raw_page_continues_even_when_projection_discards_items() {
        let mut pages = Vec::new();
        let result = collect_release_pages(|page| {
            pages.push(page);
            std::future::ready(Ok::<_, ()>((vec![page], if page < 3 { 50 } else { 9 })))
        })
        .await
        .unwrap();
        assert_eq!(pages, [1, 2, 3]);
        assert_eq!(result, [1, 2, 3]);
    }

    #[tokio::test]
    async fn exact_multiple_requires_the_final_empty_page() {
        let mut pages = Vec::new();
        collect_release_pages(|page| {
            pages.push(page);
            std::future::ready(Ok::<_, ()>((
                Vec::<()>::new(),
                if page < 3 { 50 } else { 0 },
            )))
        })
        .await
        .unwrap();
        assert_eq!(pages, [1, 2, 3]);
    }

    #[tokio::test]
    async fn later_failure_does_not_publish_a_partial_snapshot() {
        let result = collect_release_pages(|page| {
            std::future::ready(if page == 1 {
                Ok((vec![1], 50))
            } else {
                Err("page failed")
            })
        })
        .await;
        assert_eq!(result, Err("page failed"));
    }
}
