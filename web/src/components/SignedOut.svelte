<script lang="ts">
  // A page without the token; nothing to load.
  import OrbitMark from "./OrbitMark.svelte";
</script>

<section class="signed-out">
  <OrbitMark size={280} />
  <div class="copy">
    <p class="code bitmap">Error 401</p>
    <h1>Not signed in</h1>
    <p>
      This page has no key to your Getman. Open the link Getman printed in your terminal when it
      started; it ends in <code>#token=…</code>.
    </p>
    <p class="muted">
      Getman checks that key on every call, so other web pages cannot use it to send requests or
      read your files.
    </p>
  </div>
</section>

<style>
  .signed-out {
    display: grid;
    align-content: center;
    justify-items: center;
    gap: var(--s5);
    height: 100%;
    padding: var(--s6);
    overflow: auto;
    text-align: center;
  }

  .copy {
    display: grid;
    justify-items: center;
    gap: var(--s3);
    max-width: 56ch;
  }

  .code {
    margin: 0;
    padding: 4px 12px;
    border-radius: var(--r-sm);
    background: var(--error-container);
    color: var(--on-error-container);
  }

  h1 {
    margin: 0;
    font-size: var(--fs-headline);
    font-weight: 400;
    line-height: 1.25;
  }

  p {
    margin: 0;
  }

  code {
    font-family: var(--font-code);
  }

  @media (max-height: 560px) {
    .signed-out :global(.orbit-mark) {
      display: none;
    }
  }
</style>
