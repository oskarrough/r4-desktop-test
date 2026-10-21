{
  "base": "{{ mode }}",
  "colors": {
    "window": "{{ background }}",
    "outline": "{{ mix background foreground 11% }}",
    "surface_hover": "{{ mix background foreground 5% }}",
    "surface_active": "{{ mix background foreground 9% }}",
    "text": "{{ foreground }}",
    "secondary": "{{ mix background foreground 56% }}",
    "dim": "{{ mix background foreground 26% }}",
    "surface": "{{ mix background foreground 6% }}",
    "edge": "{{ mix background foreground 13% }}",
    "accent": "{{ accent }}"
  }
}
