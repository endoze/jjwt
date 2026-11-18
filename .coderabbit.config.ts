import { defineConfig, mergeConfig, type CodeRabbitContext } from "@coderabbitai/config"
// Shared settings live in YAML so they match the other crates' .coderabbit.yaml.
// A root .coderabbit.yaml would override this file entirely and drop the
// trust gate below, so the YAML uses a name CodeRabbit does not auto-load.
import base from "./.coderabbit.base.yaml"

/** Repository associations that carry push access on a personal-account repo. */
const TRUSTED_ASSOCIATIONS = ["OWNER", "COLLABORATOR"]

/** True when the PR author can push to this repository. Fails closed when unknown. */
function authorCanPush(ctx: CodeRabbitContext): boolean {
  const association = ctx.pr?.authorAssociation
  return association != null && TRUSTED_ASSOCIATIONS.includes(association)
}

export default defineConfig((ctx: CodeRabbitContext) => {
  const trusted = authorCanPush(ctx)

  return mergeConfig(base, {
    reviews: {
      auto_review: {
        enabled: trusted,
      },
      // `@coderabbitai review` bypasses `auto_review.enabled`, so untrusted PRs
      // also get an empty file set to leave nothing for a manual review to cover.
      path_filters: trusted ? [] : ["!**"],
    },
    chat: {
      auto_reply: trusted,
    },
  })
})
