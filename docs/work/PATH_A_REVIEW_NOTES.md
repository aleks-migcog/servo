# Path A Review Notes

## Thumb parentage

The current range UA shadow tree is `input -> ::slider-track -> (::slider-fill, ::slider-thumb)`.
Path A now positions the thumb relative to the track content box, including negative block-axis
offsets when the thumb is taller than the track.

TODO follow-up: consider whether `::slider-thumb` should become a direct child of the range input.
That would simplify frame-level positioning because the fill could remain track-relative while the
thumb would be positioned directly in the range coordinate space. This is not a blocker for the
current iteration.
