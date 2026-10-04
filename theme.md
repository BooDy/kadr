## Kadr UI/UX Theme Specifications

### Color System

| Role | Hex Code | Application |
| --- | --- | --- |
| **Primary Background** | `#2A0A0A` | Main application canvas, media library grids, and video player background. |
| **Secondary Background** | `#3B1111` | Navigation sidebars, modal dialogs, and content panels to establish depth. |
| **Radiant Accent** | `#FF6B00` | Active states, selected navigation tabs, and media playback progress bars. |
| **Golden Highlight** | `#FFB800` | Critical information badges, warnings, focus rings for 10-foot UI, and volume sliders. |
| **Accent Red** | `#D43F15` | Primary calls to action, such as "Add Library" or "Play/Resume" buttons. |
| **Text Primary** | `#F5E5E5` | Body copy, primary movie titles, and active navigation labels. |
| **Muted Elements** | `#5C3C3C` | Inactive icons, secondary metadata (release year, genre), and disabled states. |

### Typography

* **Primary Interface:** Use **Inter** or **Geist** for clean, scalable readability from mobile devices to large living room displays.
* **Multilingual & RTL Support:** Implement **IBM Plex Sans** (specifically the Arabic variant) or **Noto Sans** as a fallback stack to ensure seamless rendering of Right-to-Left text in media descriptions.
* **Technical Data:** Use **JetBrains Mono** or **Fira Code** for server health readouts, console logs, bitrate statistics, and file path displays.
* **Hierarchy:** Maintain weight consistency by using Regular (400) for body text, Medium (500) for UI labels, and Semi-Bold (600) for prominent headers. When displaying long metadata strings (such as extensive cast lists for films like *The Battle of Algiers* or *Network*), ensure text truncates cleanly with an ellipsis rather than wrapping and breaking the grid layout.

### Iconography

* **Libraries:** Utilize **Phosphor Icons** or **Lucide** for comprehensive, consistently weighted icon sets.
* **Styling:** Select icons featuring soft, rounded caps and joins. This visually complements the fluid, overlapping geometry of the Kadr logo.
* **Interactive States:** Render inactive menu items as Outlined (Stroked) icons in `#5C3C3C`. Transition these to Solid (Filled) icons colored in `#FF6B00` when the user selects or focuses on the respective view.

### Component Architecture & Interactions

* **Geometry & Radii:** Apply an `8px` to `12px` border radius across all primary UI components, including movie poster cards, buttons, and input fields, to avoid harsh edges.
* **Elevation (Dark Mode Depth):** Avoid traditional drop shadows, which fail to render effectively against a `#2A0A0A` background. Define spatial depth by applying a 1px semi-transparent inner border (e.g., `rgba(245, 229, 229, 0.08)`) to overlapping elements like dropdown menus, context menus, or settings modals.
* **10-Foot UI Focus Rings:** For users navigating via a television remote or gamepad, active focus states must be instantly identifiable from across the room. Apply a high-contrast `3px` solid border using the `#FFB800` Golden Highlight around currently focused media cards.
* **Pointer Hover States:** When interacting via a mouse, lift interactive elements by lightening their specific background container by 5-10% and subtly revealing the `#D43F15` Accent Red on hover-triggered action overlays.
