---
title: hard terrain plan
created: 2026-05-09
tags:
  - terrain
---

# Terrain 

## Generation
Ill start out with a noise function that is always > 0.

## Rendering
I should be able to use the same heightmap setup as for the water.
Plus the same shader should be applicable without change.

## Inclusion into the water simulation
This will be the difficult portion.
If I would calculate the water height as terrain + water, the water rendering breaks, since it assumes that the heightmap is the full height to be rendered.
Ill interpret the terrain + water combination in the following way: if terrain > water => there is no water and I can set the water heightmap to -1 or similar fixed values.
The water flow calculation uses max(water, terrain).
The flow application sets water to terrain + flow amount, if terrain > water.

## Physics

Using avian with the heightmap should be straight forward.


