# Manual tool changes

Multiple tools can be planned and simulated without an ATC. NC export additionally
needs a tool-change configuration supported by the controller.

In Setup → Machine → Tool changes, choose manual changes and set the change
position in **work coordinates**. Its Z must be at or above travel clearance.
The post checks the approach/return moves against configured travel limits and
fixture envelopes. Tool changes and operator waiting time are excluded from the
cutting-path playback timeline.

Assign each tool a distinct tool number and a calibrated H length-offset register
in the tool library. These are controller measurements, not the catalogue's nominal
overall length. Load the first tool before starting the program; the program enables
its H register before its first positioning move.

For subsequent changes the program retracts to clearance, stops spindle and coolant
(M5/M9), raises to change Z and moves to change X/Y. Generic metric and Mach3 then
select T and issue a mandatory **M0**, followed by G43 H for the replacement tool,
restoration of metric/absolute/feed/work-offset modes, clearance and the validated cutting-path XY position. The operator
swaps the tool and resumes. M1 is not used: optional stops can be disabled.

For LinuxCNC, enable the configured manual M6 handshake (`hal_manualtoolchange` or
an equivalent safe controller routine). The post issues Tn M6 at the configured
position, then G43 Hn; no ATC pocket or magazine is needed. A controller routine that
moves axes must manage its own safe motion, as for automatic M6 routines.
M0 is not used as a request to jog, probe, or change work zero: LinuxCNC does not allow
MDI/manual actions during an Auto-mode M0 pause.

GRBL supports neither M6 nor H-register tool compensation. Multi-tool path planning
still works, but this post requires one tool per exported program, with tool setting
between programs. It does not emit unsupported tool-change/compensation commands.

Sources:

- [LinuxCNC M0/M1 and M6 documentation](https://linuxcnc.org/docs/stable/html/gcode/m-code.html)
- [Official LinuxCNC documentation source](https://github.com/LinuxCNC/linuxcnc/blob/master/docs/src/gcode/m-code.adoc)
- [LinuxCNC tool-length compensation](https://linuxcnc.org/docs/stable/html/gcode/g-code.html#gcode:g43)

- [Official GRBL supported-command list](https://github.com/gnea/grbl#supported-g-codes-in-grbl-v11)
