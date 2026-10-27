# Required follow-up for Keep

This review runs roughly weekly. If the maintainer chooses **Keep** because no
implementation correction is needed, a follow-up update to the reviewed source
file is still required. Commit an update such as a review comment recording the
decision and its rationale, with a link to this issue, so the file's Git
last-change date advances. Otherwise the next weekly invocation will select the
same file again. Updating only this issue or the filesystem modification time
does not affect `.local/bin/old-source`, which uses Git history. Record the
follow-up commit in this issue before closing it. The investigation skill does
not make this source update; it is carried out after the maintainer's decision.
