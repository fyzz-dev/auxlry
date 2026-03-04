import { createRootRoute, Link, Outlet, useMatches } from "@tanstack/react-router";
import { LayoutDashboard, Brain, Settings } from "lucide-react";
import {
  Sidebar,
  SidebarContent,
  SidebarFooter,
  SidebarGroup,
  SidebarGroupContent,
  SidebarGroupLabel,
  SidebarHeader,
  SidebarInset,
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  SidebarProvider,
  SidebarTrigger,
} from "@/components/ui/sidebar";
import { Separator } from "@/components/ui/separator";
import {
  Breadcrumb,
  BreadcrumbItem,
  BreadcrumbList,
  BreadcrumbPage,
  BreadcrumbSeparator,
} from "@/components/ui/breadcrumb";
import { useStatus } from "@/lib/queries";

const NAV = [
  { to: "/", label: "Overview", icon: LayoutDashboard },
  { to: "/memory", label: "Memory", icon: Brain },
  { to: "/config", label: "Config", icon: Settings },
] as const;

function StatusIndicator() {
  const { data, isError } = useStatus();
  const isOnline = data?.status === "running" && !isError;

  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <SidebarMenuButton size="sm" className="cursor-default" tooltip="System status">
          <span
            className={`size-2 shrink-0 rounded-full ${isOnline ? "bg-emerald-500 animate-pulse" : "bg-destructive"}`}
          />
          <span className="text-xs text-muted-foreground">
            {isOnline ? "Online" : "Offline"}
          </span>
        </SidebarMenuButton>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}

function AppSidebar() {
  return (
    <Sidebar variant="inset" collapsible="icon">
      <SidebarHeader>
        <SidebarMenu>
          <SidebarMenuItem>
            <SidebarMenuButton size="lg" asChild>
              <Link to="/">
                <div className="bg-sidebar-primary text-sidebar-primary-foreground flex aspect-square size-8 items-center justify-center rounded-lg text-sm font-bold">
                  ax
                </div>
                <div className="grid flex-1 text-left text-sm leading-tight">
                  <span className="truncate font-semibold">auxlry</span>
                  <span className="truncate text-xs text-muted-foreground">
                    dashboard
                  </span>
                </div>
              </Link>
            </SidebarMenuButton>
          </SidebarMenuItem>
        </SidebarMenu>
      </SidebarHeader>
      <SidebarContent>
        <SidebarGroup>
          <SidebarGroupLabel>Navigation</SidebarGroupLabel>
          <SidebarGroupContent>
            <SidebarMenu>
              {NAV.map(({ to, label, icon: Icon }) => (
                <SidebarMenuItem key={to}>
                  <SidebarMenuButton asChild tooltip={label}>
                    <Link to={to} activeProps={{ className: "font-medium" }}>
                      <Icon />
                      <span>{label}</span>
                    </Link>
                  </SidebarMenuButton>
                </SidebarMenuItem>
              ))}
            </SidebarMenu>
          </SidebarGroupContent>
        </SidebarGroup>
      </SidebarContent>
      <SidebarFooter>
        <StatusIndicator />
      </SidebarFooter>
    </Sidebar>
  );
}

function RootLayout() {
  const matches = useMatches();
  const current = matches[matches.length - 1];
  const pageLabel =
    NAV.find(
      (n) =>
        n.to === current?.pathname?.replace(/\/$/, "") ||
        (n.to === "/" && current?.pathname === "/"),
    )?.label ?? "Dashboard";

  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset>
        <header className="flex h-12 shrink-0 items-center gap-2 border-b px-4">
          <SidebarTrigger className="-ml-1" />
          <Separator orientation="vertical" className="mr-2 h-4" />
          <Breadcrumb>
            <BreadcrumbList>
              <BreadcrumbItem>Dashboard</BreadcrumbItem>
              <BreadcrumbSeparator />
              <BreadcrumbItem>
                <BreadcrumbPage>{pageLabel}</BreadcrumbPage>
              </BreadcrumbItem>
            </BreadcrumbList>
          </Breadcrumb>
        </header>
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}

export const Route = createRootRoute({
  component: RootLayout,
});
