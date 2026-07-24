---
aside: false
outline: false
title: OpenAPI operation
description: Generated request, response, and schema reference for an Oore CI API operation.
---

<script setup lang="ts">
import { useRoute } from 'vitepress'

const route = useRoute()

const operationId = route.data.params.operationId
</script>

<h1>{{ route.data.params.pageTitle }}</h1>

<ClientOnly>
  <OAOperation :operationId="operationId" />
</ClientOnly>
