//! The restricted Damerau-Levenshtein distance, also known as the optimal
//! string alignment distance.
//!
//! Four edits cost one each: inserting an element, deleting one, substituting
//! one for another, and transposing two adjacent ones. The distance is the
//! cheapest sequence of edits turning one sequence into the other.
//!
//! "Restricted" is the transposition's small print: a substring may be edited
//! only once, so a pair that was swapped cannot then be edited again.
//!
//! ```
//! use spell_distance::distance;
//!
//! // One transposition, the edit plain Levenshtein has to pay twice for
//! assert_eq!(distance("teh", "the"), 1);
//!
//! // Unrestricted, `CA` -> `AC` -> `ABC` is two edits. Restricted, the
//! // swapped pair cannot then be split by an insertion, so the cheapest
//! // alignment substitutes and inserts instead: three
//! assert_eq!(distance("CA", "ABC"), 3);
//! ```
//!
//! That restriction is what keeps the cost of a cell local: every cell reads
//! only the two rows above it, so the whole matrix is never held at once.
//!
//! The result is symmetric and zero only for equal sequences, but it is not a
//! metric - the triangle inequality fails, `CA` to `ABC` being the example
//! above against `CA` to `AC` to `ABC`. Structures that assume a metric, a
//! BK-tree among them, cannot be built on it unchecked.

mod tests;

/// Returns the number of edits between `left` and `right`, comparing them as
/// Unicode scalar values rather than bytes, so an `é` is one element and not
/// the two its UTF-8 encoding takes.
///
/// # Examples
///
/// ```
/// use spell_distance::distance;
///
/// assert_eq!(distance("kitten", "sitting"), 3);
/// assert_eq!(distance("naïve", "naive"), 1);
/// assert_eq!(distance("sea", "sea"), 0);
/// ```
pub fn distance(left: &str, right: &str) -> usize {
    let left: Vec<char> = left.chars().collect();
    let right: Vec<char> = right.chars().collect();

    distance_of(&left, &right)
}

/// Returns the number of edits between `left` and `right`.
///
/// Works on any sequence whose elements can be compared, characters being only
/// the common case: bytes, tokens, or whole words work the same way.
///
/// # Examples
///
/// ```
/// use spell_distance::distance_of;
///
/// let left = ["by", "the", "sea", "shore"];
/// let right = ["by", "the", "shore", "sea"];
///
/// // The last two words changing places is one edit
/// assert_eq!(distance_of(&left, &right), 1);
/// ```
pub fn distance_of<T: PartialEq>(left: &[T], right: &[T]) -> usize {
    // Every element of the other sequence has to be inserted or deleted, and
    // the loop below would have nothing to compare against
    if left.is_empty() || right.is_empty() {
        return left.len().max(right.len());
    }

    let width = right.len() + 1;
    // Three rows of the matrix, one per row a cell reads: the two above it and
    // the one being filled. Holding the whole matrix would cost the product of
    // the two lengths for a number only its last cell carries
    let mut before_previous = vec![0; width];
    // Row zero: turning an empty prefix of `left` into a prefix of `right` is
    // one insertion per element
    let mut previous: Vec<usize> = (0..width).collect();
    let mut current = vec![0; width];

    for (i, left_element) in left.iter().enumerate() {
        // Turning a prefix of `left` into the empty prefix of `right` is one
        // deletion per element
        current[0] = i + 1;

        for (j, right_element) in right.iter().enumerate() {
            let substitution = usize::from(left_element != right_element);

            let mut cost = (previous[j] + substitution)
                .min(previous[j + 1] + 1) // Delete `left_element`
                .min(current[j] + 1); // Insert `right_element`

            // Both elements match the other's predecessor, so the pair is a
            // transposition - charged from two rows and two columns back,
            // which is the alignment before either element was touched
            if i > 0
                && j > 0
                && left_element == &right[j - 1]
                && &left[i - 1] == right_element
            {
                cost = cost.min(before_previous[j - 1] + 1);
            }

            current[j + 1] = cost;
        }

        // The row just filled becomes the row above, the one above it the row
        // two up, and the row it displaces is overwritten cell by cell before
        // anything reads it again
        std::mem::swap(&mut before_previous, &mut previous);
        std::mem::swap(&mut previous, &mut current);
    }

    // The last row filled, at its last column: both sequences consumed whole
    previous[right.len()]
}
