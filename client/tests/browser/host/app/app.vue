<template>
  <!-- A console says everything it and its bundles have to tell on the one island at the top of the screen and
       stands no second road for news, so its toaster is not stood up at all (bmo-ui's message island).
       The host is the screen's console here: it roots the island the screen raises its notices on, which is
       where a reader, and a check, reads what the screen has to say, and leaves the library's toaster down. -->
  <UApp :toaster="null">
    <NuxtPage />
    <MessageIsland />
  </UApp>
</template>
