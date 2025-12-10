#include "include/glue.hpp"
#include "mapnik/projection.hpp"
#include <memory>

static bool is_mapnik_setup = false;

void setup_mapnik(const std::string& datasources_dir, const std::string& fonts_dir) {
  if (is_mapnik_setup) return;
  std::cerr << "Setting up mapnik..." << std::endl;
  mapnik::setup();
  mapnik::logger::set_severity(mapnik::logger::severity_type::debug);
  mapnik::logger::use_console(); // TODO: pipe through file

  std::cerr << "Registering resources..." << std::endl;
  mapnik::datasource_cache::instance().register_datasources(datasources_dir);
  mapnik::freetype_engine::register_fonts(fonts_dir);

  is_mapnik_setup = true;
}

std::unique_ptr<MapRenderer> new_MapRenderer(uint32_t width, uint32_t height, const std::string& map_def_file, std::shared_ptr<cairo_t> cairo, const std::string& base_path) {
  return std::make_unique<MapRenderer>(width, height, map_def_file, cairo, base_path);
}

std::unique_ptr<MapRenderer> new_MapRendererFromFile(uint32_t width, uint32_t height, const std::string& map_def_path, std::shared_ptr<cairo_t> cairo, const std::string& base_path) {
  fs::path path = fs::path(map_def_path);
  return std::make_unique<MapRenderer>(width, height, path, cairo, base_path);
}

std::shared_ptr<box2d_double> new_box2d_double(double startx, double starty, double endx, double endy) {
  return std::make_shared<box2d_double>(startx, starty, endx, endy);
}

struct cairo_closer {
  void operator()(cairo_t* cairo) {
    if (cairo) cairo_destroy(cairo);
  }
};

std::shared_ptr<cairo_t> make_cairo_shared(cairo_t* cr) {
  return std::shared_ptr<cairo_t>(cr, cairo_closer{});
}

std::shared_ptr<mapnik::geometry::point<double>> new_point_double(double x, double y) {
  return std::make_shared<mapnik::geometry::point<double>>(x, y);
}

std::shared_ptr<mapnik::projection> new_projection(const std::string& srs) {
  return std::make_shared<mapnik::projection>(srs);
}

std::unique_ptr<std::string> projection_definition(std::shared_ptr<mapnik::projection> proj) {
  return std::make_unique<std::string>(proj->definition());
}

std::shared_ptr<mapnik::box2d<double>> make_center_box(
   const mapnik::geometry::point<double>& center,
   const mapnik::projection& projsrc,
   const mapnik::projection& projdst,
   double projected_units_per_pixel,
   uint32_t screen_w, uint32_t screen_h
 ) {
   mapnik::geometry::point<double> center_transformed = center;
   mapnik::proj_transform proj_transform = mapnik::proj_transform(projsrc, projdst);
   proj_transform.forward(center_transformed);

   double w_half = (((double) screen_w) * projected_units_per_pixel) / 2.;
   double h_half = (((double) screen_h) * projected_units_per_pixel) / 2.;

   double minx = center_transformed.x - w_half;
   double maxx = center_transformed.x + w_half;
   double miny = center_transformed.y - h_half;
   double maxy = center_transformed.y + h_half;

   return std::make_shared<mapnik::box2d<double>>(minx, miny, maxx, maxy);
}

double box2d_get_startx(const box2d_double& b) {
  return b.minx();
}

double box2d_get_starty(const box2d_double& b) {
  return b.miny();
}

double box2d_get_endx(const box2d_double& b) {
  return b.maxx();
}

double box2d_get_endy(const box2d_double& b) {
  return b.maxy();
}
