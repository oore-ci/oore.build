---
aside: false
outline: false
title: vitepress-openapi
description: Interactive documentation for an oore.build API operation — request parameters, response schemas, and live playground.
---

<script setup lang="ts">
import { useRoute } from 'vitepress'

const route = useRoute()

const operationId = route.data.params.operationId
</script>

<OAOperation :operationId="operationId" />
