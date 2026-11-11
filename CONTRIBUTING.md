# Reporting issues and suggesting changes

Use [New issue](https://github.com/Noob-P2W/OpenSourceCAM/issues/new/choose)
to choose **Bug report / fix needed**, **Suggestion / feature request**, or
**Question / help**. A blank issue is also available. Suggestions are welcome;
opening an issue does not promise implementation or a delivery date.

For bugs, include reproduction steps, the app build ID, and your device/browser
when possible. Screenshots and a small public example project can help.

## Labels and decisions

- `bug`: incorrect behavior or a fix needed.
- `enhancement`: a suggestion, feature request or improvement.
- `question`: a question or help request.
- `needs triage`: new report awaiting review; applied by the issue forms.
- `needs information`: the maintainer needs more details to investigate.
- `in progress`: work is actively underway.
- `fixed`: implemented; link the fixing commit or pull request and close as completed.
- `wontfix`: reviewed but intentionally not planned; explain why and close as not planned.
- `duplicate`: link the original issue, then close the duplicate as not planned.

Maintainers apply progress and decision labels. Remove obsolete status labels as
an issue progresses. A suggestion can remain open without being scheduled.
Only mark a bug fixed once the fix exists, with its publication status stated
when relevant. These labels do not automatically close issues or promise work.

`.github/labels.json` lists the intended labels. The **Set up issue labels**
workflow adds missing labels on main or when manually run, preserving existing
labels, descriptions and colors. It never deletes labels or relabels old issues.
GitHub labels work for this repository without organization-level issue types.
