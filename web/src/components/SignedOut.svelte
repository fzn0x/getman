<script lang="ts">
  // A page without the token; nothing to load.
</script>

<section class="signed-out">
  <p class="bitmap code">401</p>
  <h1 class="bitmap">Not signed in</h1>
  <p>
    This page has no key to your Getman. Open the link Getman printed in your terminal when it
    started; it ends in <code>#token=…</code>.
  </p>
  <p class="muted">
    Getman checks that key on every call, so other web pages cannot use it to send requests or read
    your files.
  </p>
</section>

<style>
  .signed-out {
    display: grid;
    align-content: center;
    gap: var(--s3);
    height: 100%;
    max-width: 64ch;
    margin: 0 auto;
    padding: var(--s6);
  }

  .code {
    margin: 0;
    font-size: var(--b8);
    color: var(--accent);
  }

  h1 {
    margin: 0;
    font-size: var(--b3);
    font-weight: 400;
  }

  p {
    margin: 0;
  }

  code {
    font-family: var(--font-code);
  }
</style>
