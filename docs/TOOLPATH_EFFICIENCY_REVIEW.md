# Generated toolpath efficiency review

Reviewed 21 November 2026 against `73c2982`. All 20 strategy IDs were generated
and posted, including saved legacy strategies. Four additional fixtures exercise
selected planar boundaries and consecutive clearing operations. The public
fixtures are original geometry; the supplied private model is not included.

## Changes

- Planar offsets measure horizontal, vertical and diagonal distance in physical
  units rather than shortening straight-wall stepover by a square-grid factor.
  A bounded weighted distance field reduces excess laps. A cell-diagonal margin
  keeps rounding within the requested maximum. Cutter dilation, the
  50% stepover cap, protected islands and original clearance checks remain. The
  3D contact field retains its earlier conservative erosion spacing.
- Numeric outside clearing uses the same conservative air-span trimming as planar
  clearing. Complete oversized laps outside the physical blank are omitted; the
  retained rectangle and circumscribed corners are unchanged. Requested finishing
  feeds and spring passes remain explicit.
- Long empty links compare feeding the existing contour with a certified lift,
  rapid and local feed approach. A floor-level feed shortcut remains available
  only when the full tool and fixture route is clear. Unknown or exhausted proofs
  retain the original obstacle-safe route. A short link is not lifted merely to
  achieve a shorter geometric distance.
- 3D offset clearing now shares this trimming/linking path and nearby entry
  selection. It also respects same-tool operation transitions.
- Prior enabled, whole-stock facing supplies the automatic starting depth of
  subsequent roughing. Planar roughing skips completed faced layers; 3D roughing
  includes that start depth when calculating every level and reaches its intended
  floor. Model openings and externally prepared-stock assertions do not seed
  cleared material into the certificate.
- Optimized bidirectional facing can use the longer stock axis and starts near
  the previous position. It checks the complete alternate sweep and overrun
  against fixtures, and retains X passes when Y would hit a clamp. Explicit
  Positive X and Negative X rasters keep their chosen direction.
- Implicit Optimized clearance now builds the fixture height tree too. A hidden
  fixture is included even when Remaining-stock clearance is blank.

The changes run in the shared Rust generator before postprocessing. Native
G2/G3 spans retain their original geometry and indices; the NC path is not
shortened independently of the stock simulation. Tools, operation order,
allowances, cutting directions, entry ramp angles, helical pitches, lead moves,
finishing/spring passes, drilling R planes and tool-change clearance contracts
are preserved. Existing machine/fixture preflight remains enabled.

## Repeatable measurements

Run `cargo run --locked --no-default-features --example toolpath_audit` for the
public fixture set, or append a local project filename for that job. Measurements
use programmed cutting/plunge/rapid feeds; they exclude acceleration, spindle
run-up and controller-specific cycle overhead. They are estimates, not measured
machine cycle times. The parallel-finishing fixture includes its preceding 3D roughing operation,
so its reduction below is a sequence improvement rather than a faster finish cut.

| Strategy / fixture | Before, s | After, s | Reduction |
| --- | ---: | ---: | ---: |
| 2D pocket clearing (rectangular) | 143.61 | 143.61 | 0.0% |
| 2D contour (external) | 52.20 | 52.20 | 0.0% |
| 2D contour (internal) | 39.46 | 39.46 | 0.0% |
| Peck drilling | 6.01 | 6.01 | 0.0% |
| Drilling | 5.24 | 5.24 | 0.0% |
| Face milling | 815.82 | 774.99 | 5.0% |
| Circular pocket clearing | 49.17 | 49.17 | 0.0% |
| Outside clearing (numeric) | 1288.50 | 917.44 | 28.8% |
| 3D offset clearing | 1783.67 | 969.67 | 45.6% |
| 3D rough + parallel finish | 2713.73 | 1900.82 | 30.0% |
| Trochoidal pocket milling (legacy) | 2222.94 | 2222.94 | 0.0% |
| Slot milling | 27.58 | 27.58 | 0.0% |
| Trace milling (legacy) | 27.58 | 27.58 | 0.0% |
| Bore finishing | 27.80 | 27.80 | 0.0% |
| Circular milling | 25.13 | 25.13 | 0.0% |
| Thread milling | 24.99 | 24.99 | 0.0% |
| Contour engraving | 8.20 | 8.20 | 0.0% |
| Chamfer milling | 8.15 | 8.15 | 0.0% |
| Planar finishing | 68.96 | 68.96 | 0.0% |
| 2D offset clearing | 172.43 | 137.17 | 20.4% |
| Outside clearing (selected plane) | 991.59 | 777.17 | 21.6% |
| Outside clearing (two operations) | 1370.82 | 1085.91 | 20.8% |
| 2D offset clearing (selected plane) | 172.43 | 137.17 | 20.4% |
| 2D offset clearing (two operations) | 269.09 | 222.23 | 17.4% |

The supplied model's selected outside-clearing case improved from 1173.67 s to
993.37 s (15.4%), with the same final plane. Its private geometry remains local.
The numeric outside test independently compares final sampled stock with the
untrimmed cycle and checks low XY rapids against only earlier cutter sweeps.
Fixture regressions cover an intervening hidden clamp and an obstruction to the
alternate facing axis. All strategy cases also post through NC preflight and
check native arcs. Dense probes verify rotated planar boundaries and islands,
and successive offsets on straight and rotated walls stay within the requested
maximum stepover.
Existing suites verify retained features, surface contact,
coverage, directions, entries, allowances, machine bounds and controller output.

An unchanged row means this review found no justified cycle shortening in that
fixture: intentional wall finishing, spring passes, thread pitch, lead geometry
and drilling retracts are not redundant air clearing. This is not a guarantee of
globally minimal motion for arbitrary parts or controllers. Stock proofs remain
bounded and conservative; uncertain residual regions retain their cuts and links.

## External reference checks

- [Autodesk: reducing air cutting](https://help.autodesk.com/cloudhelp/ENU/Fusion-CAM/files/REDUCE-AIR-CUTTING-POCKET.htm): actual stock extent is needed to distinguish engagement from unnecessary cutting motion.
- [Harvey Performance: climb and conventional milling](https://www.harveyperformance.com/in-the-loupe/conventional-vs-climb-milling/): cutting direction is a machining choice; shortening links must not reverse closed cutting contours or replace explicit user choices.
- [Fusion community: How to force tool to stay down?](https://forums.autodesk.com/t5/fusion-manufacture-forum/how-to-force-tool-to-stay-down/td-p/14049374): stock/boundary selection and linking behavior need separate examination. A stay-down request alone does not prove a direct route safe.

These references were read for requirements. No reference code was copied. See
[the machining-method audit](TOOLPATH_AUDIT.md) for strategy-specific technology
limits and [remaining stock and arcs](REMAINING_STOCK_AND_ARCS.md) for the shared
certificate and NC contracts.
