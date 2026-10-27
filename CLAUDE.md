# mic-rtsim

## Landing

Every branch lands through a pull request, so that Graphide's review runs first. You still land it
yourself.

1. Finish the branch as usual: merge `origin/main`, look at `git status` for `UU`, and run the
   tests the change needs.
2. Open the pull request and wait for the review, as a background task:

   ```
   /home/greencheetah/Projects/graphide/graphide/utilities/scripts/pr-review.sh \
     --root <worktree> --base main --body-file <file>
   ```

   It pushes the branch, opens the pull request, waits for the "Graphide review" check run, and
   prints the verdict and the findings. End the body file with
   `🤖 Generated with [Claude Code](https://claude.com/claude-code)`.
3. On exit 0, read the verdict and every finding.
   - Fix the real ones. A bug gets a failing test first.
   - Push, then run the script again for the next round.
   - Answer each false positive in the pull request with a one-line reason.

   On exit 3 (no review started) or 4 (still running), land without the review and say so.
4. Land. If `main` moved, merge it again and check for `UU`, then `git push origin HEAD:main`.
   A clean merge of `main` needs no new review round. Never land while a review is still running
   on the pull request, because closing a pull request ends its unfinished reviews.
5. Delete the remote branch: `git push origin --delete <branch>`.
