pub fn paginate<T: Clone>(
    items: &[T],
    items_per_page: Option<&str>,
    page: Option<&str>,
) -> Result<(Vec<T>, i64, i64), String> {
    let item_count = items.len() as i64;
    if item_count == 0 {
        return Ok((Vec::new(), 0, 0));
    }

    let mut items_per_page_val = 100i64;
    if let Some(raw) = items_per_page {
        let parsed: u64 = raw
            .parse()
            .map_err(|_| "invalid items per page".to_owned())?;
        if parsed == 0 {
            return Err("invalid items per page".to_owned());
        }
        items_per_page_val = parsed as i64;
    }

    let mut page_num = 0i64;
    if let Some(raw) = page {
        page_num = raw.parse::<u64>().map_err(|_| "invalid page".to_owned())? as i64;
    }

    let mut page_count = item_count / items_per_page_val;
    if item_count % items_per_page_val != 0 {
        page_count += 1;
    }

    let start = (page_num * items_per_page_val).min(item_count) as usize;
    let end = ((page_num + 1) * items_per_page_val).min(item_count) as usize;

    Ok((items[start..end].to_vec(), page_count, item_count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_list() {
        let (items, page_count, item_count) = paginate(&[1, 2, 3][..0], None, None).unwrap();
        assert!(items.is_empty());
        assert_eq!(page_count, 0);
        assert_eq!(item_count, 0);
    }

    #[test]
    fn default_page_size() {
        let data: Vec<i32> = (0..150).collect();
        let (items, page_count, item_count) = paginate(&data, None, None).unwrap();
        assert_eq!(item_count, 150);
        assert_eq!(page_count, 2);
        assert_eq!(items.len(), 100);
    }
}
