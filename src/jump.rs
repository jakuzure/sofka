//! Jump codes: the part of each name on screen that sets it apart from the
//! others, so a row can be reached by typing a few of its characters.
//!
//! Generated names share long prefixes (`api-7d8f9b5c4-xk2lp`,
//! `api-7d8f9b5c4-q9wzt`), so a code skips what a name has in common with its
//! closest neighbour and starts where the two part ways. Pure; the app layer
//! freezes the codes when a jump starts and matches typed characters against
//! them.

/// Where each name's code starts, as a char offset into that name.
///
/// Codes never end up equal to, or a prefix of, another code unless the
/// names themselves leave no other choice (two identical names).
pub fn starts(names: &[&str]) -> Vec<usize> {
    let names: Vec<Vec<char>> = names.iter().map(|n| n.chars().collect()).collect();
    let others = |i: usize| (0..names.len()).filter(move |&j| j != i);
    let mut starts: Vec<usize> = (0..names.len())
        .map(|i| {
            others(i)
                .map(|j| common_prefix(&names[i], &names[j]))
                .max()
                .unwrap_or(0)
        })
        .collect();
    // A name that ends inside another one, or whose code repeats another
    // code, takes one more character from before its start until it stands
    // apart. Starts only shrink, so this ends.
    loop {
        let clashing: Vec<usize> = (0..names.len())
            .filter(|&i| {
                let code = &names[i][starts[i]..];
                starts[i] > 0 && others(i).any(|j| names[j][starts[j]..].starts_with(code))
            })
            .collect();
        if clashing.is_empty() {
            return starts;
        }
        for i in clashing {
            starts[i] -= 1;
        }
    }
}

/// How many characters of each code, from `starts`, reach that name alone:
/// one more than it shares with any other code, and at most the whole code.
pub fn lengths(names: &[&str], starts: &[usize]) -> Vec<usize> {
    let codes: Vec<Vec<char>> = names
        .iter()
        .zip(starts)
        .map(|(name, &start)| name.chars().skip(start).collect())
        .collect();
    codes
        .iter()
        .enumerate()
        .map(|(i, code)| {
            codes
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .map(|(_, other)| common_prefix(code, other) + 1)
                .max()
                .unwrap_or(1)
                .min(code.len())
        })
        .collect()
}

fn common_prefix(a: &[char], b: &[char]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What to type for each name.
    fn keys(names: &[&str]) -> Vec<String> {
        let starts = starts(names);
        names
            .iter()
            .zip(&starts)
            .zip(lengths(names, &starts))
            .map(|((name, &start), len)| name.chars().skip(start).take(len).collect())
            .collect()
    }

    #[test]
    fn identical_names_need_their_whole_code() {
        let names = ["api", "api"];
        assert_eq!(lengths(&names, &starts(&names)), [3, 3]);
    }

    #[test]
    fn generated_pod_names_jump_by_their_random_suffix() {
        let names = [
            "api-7d8f9b5c4-xk2lp",
            "api-7d8f9b5c4-q9wzt",
            "api-7d8f9b5c4-qm4bd",
            "web-5f6c7d8d9-r2kpl",
        ];
        assert_eq!(starts(&names), [14, 15, 15, 0]);
        assert_eq!(keys(&names), ["x", "9", "m", "w"]);
    }

    #[test]
    fn codes_from_different_names_can_share_their_first_characters() {
        let names = [
            "api-7d8f9b5c4-xk2lp",
            "api-7d8f9b5c4-wq9zt",
            "web-5f6c7d8d9-r2kpl",
        ];
        assert_eq!(keys(&names), ["x", "wq", "we"]);
    }

    #[test]
    fn statefulset_ordinals_jump_by_their_number() {
        let names = ["db-0", "db-1", "db-2"];
        assert_eq!(keys(&names), ["0", "1", "2"]);
    }

    #[test]
    fn a_code_never_stays_a_prefix_of_another() {
        let names: Vec<String> = (0..12).map(|i| format!("db-{i}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let keys = keys(&names);
        assert_eq!(keys[2..10], ["2", "3", "4", "5", "6", "7", "8", "9"]);
        for (i, key) in keys.iter().enumerate() {
            let code: String = names[i].chars().skip(starts(&names)[i]).collect();
            assert!(code.starts_with(key.as_str()));
            for (j, name) in names.iter().enumerate() {
                let other: String = name.chars().skip(starts(&names)[j]).collect();
                assert!(
                    i == j || !other.starts_with(key.as_str()),
                    "{key} reaches {name}"
                );
            }
        }
    }

    #[test]
    fn a_name_inside_another_name_backs_off_until_it_stands_apart() {
        let names = ["api", "api-gateway"];
        assert_eq!(keys(&names), ["i", "-"]);
    }

    #[test]
    fn a_lone_name_jumps_by_its_first_character() {
        assert_eq!(starts(&["solo"]), [0]);
        assert_eq!(keys(&["solo"]), ["s"]);
        assert!(starts(&[]).is_empty());
    }

    #[test]
    fn identical_names_keep_their_whole_name_as_code() {
        assert_eq!(starts(&["api", "api", "web"]), [0, 0, 0]);
    }

    #[test]
    fn codes_count_characters_not_bytes() {
        let names = ["café-1", "café-2"];
        assert_eq!(starts(&names), [5, 5]);
    }
}
