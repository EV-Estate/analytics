/// `(class, style-fragment)` marking a tile/bar as provisional — data drawn from a
/// not-yet-real source (under-construction holding, missing weekly estimate): dimmed
/// + hatched, so unconfirmed values never read as confirmed. Empty when `active` is
/// false. The style fragment is appended to an existing `style` attribute.
pub fn provisional(active: bool) -> (&'static str, &'static str) {
	if active {
		(
			"opacity-50",
			";background-image:repeating-linear-gradient(45deg,rgba(230,225,211,0.22) 0 5px,transparent 5px 12px)",
		)
	} else {
		("", "")
	}
}
