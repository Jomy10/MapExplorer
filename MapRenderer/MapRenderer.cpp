#include "include/MapRenderer.hpp"

#include <iostream>
#include <string>
#include <memory>

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
#include <cairo/cairo.h>
#include <mapnik/mapnik.hpp>
#include <mapnik/map.hpp>
#include <mapnik/load_map.hpp>
#include <mapnik/datasource_cache.hpp>
#include <mapnik/font_engine_freetype.hpp>
#include <mapnik/cairo/cairo_renderer.hpp>
#include <mapnik/geometry.hpp>
#include <mapnik/geometry/box2d.hpp>
#include <mapnik/proj_transform.hpp>
#include <mapnik/projection.hpp>
#pragma clang diagnostic pop

MapRenderer::MapRenderer(uint32_t width, uint32_t height, const std::string map_def, std::shared_ptr<cairo_t> cairo, fs::path base_path) {
  std::cerr << "Loading map..." << std::endl;
  this->width = width;
  this->height = height;
  this->cairo = cairo;

  mapnik::load_map_string(this->map, map_def, false, base_path.string());
  this->map.set_width(this->width);
  this->map.set_height(this->height);
};

MapRenderer::MapRenderer(uint32_t width, uint32_t height, fs::path map_def_file, std::shared_ptr<cairo_t> cairo, fs::path base_path) {
  std::cerr << "Loading map..." << std::endl;
  this->width = width;
  this->height = height;
  this->cairo = cairo;

  mapnik::load_map(this->map, map_def_file, false, base_path.string());
  this->map.set_width(this->width);
  this->map.set_height(this->height);
};

void MapRenderer::render(void) {
  auto renderer = mapnik::cairo_renderer<std::shared_ptr<cairo_t>>(this->map, this->cairo /* scale, offset_x, offset_y */);
  renderer.apply();
}

// Controls //

void MapRenderer::move(double x, double y) {
  double aspect = ((double)this->width) / ((double)this->height);
  double startx = x;
  double endx = x + aspect;
  double starty = y;
  double endy = y + 1/aspect;

  mapnik::box2d<double> box = mapnik::box2d<double>(
    startx, starty,
    endx, endy
  );

  this->map.zoom_to_box(box);
}

void MapRenderer::zoom(double startx, double starty, double endx, double endy) {
  mapnik::box2d<double> box = mapnik::box2d<double>(
    startx, starty,
    endx, endy
  );

  this->map.zoom_to_box(box);
}

void MapRenderer::zoom_to_box(const mapnik::box2d<double>& bbox) {
  this->map.zoom_to_box(bbox);
}

void MapRenderer::resize(uint32_t width, uint32_t height) {
  this->map.set_width(width);
  this->map.set_height(height);
  this->width = width;
  this->height = height;
}

void MapRenderer::set_cairo(std::shared_ptr<cairo_t> cr) noexcept {
  this->cairo = cr;
}
