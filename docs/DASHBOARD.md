The dashboard will be a vite/react/typscript SPA bundled into the rust binary with rust-embed. We will use these libraries/tools.

- bun, bunx. Under no circumstance will we use npm, npx, or pnpm.

- shadnui / Tailwind v4: All styling should be done with the shadcn theme in mind, dont do usless work by manually choosing colors or styling that is already defined in the theme. Also all components should be added with `bunx shadcn@latest add <component>` do not ever remake a component that already exists in the shadcn library that is also usless work that often breaks things.

- react query (tanstack query) for caching requests, and react router (tanstack router) for routing. We will not be using # based routing we want full routing out of the box.

- motion.js (not framer motion) motion.js is the new package for what was framer motion. This will be used to give life to our dashboard with subtle and tasteful animation. We dont want to over do it but we dont want it static feeling either.

Again this is a SPA that will be built bundled and serverd by our rust core binary. It is not seperate, the core will be our backend and the api defined in rust will be how we get data. That means we dont need a sperate server.

Please use typscript best practices, define types when not not implicit. Do not use any, types should be defined or infered. Each ts or tsx file should have one logical component not one monolithic file doing everything. And use rust best practices. Keep components small, focused on a single responsability and reusable. Make good use of generics when applicable and use descriptive names for components, functions, and variables

## The dashboard

Please create a streamlined dashboard using shadcns sidebar components fully. Please research their implementation to do it corrently. We want the inset varient with a header with the SidebarTrigger and a breadcrumb to be on each page.

overview page as the landing page. It should feature statistics and charts (shadcn reactcharts). A line graph of memory actions taken like save,accessed and so on, to provide a metric of how active the memory is day/day.
We should have a double line graph by day on synapse/operator spawn events. We should have a heatmap of chat messages per day spanning the month. and a pie chart of current stores memory by category.

We want a memory page, that showcases the current state of the knowlege graph in memory with edges and memories defined in a visually appealing way, with physics like clumping together similar groups and pushing away from non similar groups, keep this animated and fun. It should have a mobile friendly search bar anchored closed to the bottom and center for filtering the visible memories.
