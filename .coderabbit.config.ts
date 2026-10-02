import { defineConfig, type CodeRabbitContext } from "@coderabbitai/config"

/** Repository associations that carry push access on a personal-account repo. */
const TRUSTED_ASSOCIATIONS = ["OWNER", "COLLABORATOR"]

/** True when the PR author can push to this repository. Fails closed when unknown. */
function authorCanPush(ctx: CodeRabbitContext): boolean {
  const association = ctx.pr?.authorAssociation
  return association != null && TRUSTED_ASSOCIATIONS.includes(association)
}

export default defineConfig((ctx: CodeRabbitContext) => {
  const trusted = authorCanPush(ctx)

  return {
    reviews: {
      auto_review: {
        enabled: trusted,
        // Stacked PRs target each other's branches, not just the default branch.
        base_branches: [".*"],
      },
      // `@coderabbitai review` bypasses `auto_review.enabled`, so untrusted PRs
      // also get an empty file set to leave nothing for a manual review to cover.
      path_filters: trusted ? [] : ["!**"],
    },
    chat: {
      auto_reply: trusted,
    },
  }
})
