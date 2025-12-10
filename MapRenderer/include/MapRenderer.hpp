#pragma once

#include <string>
#include <memory>
#include <filesystem>

#pragma clang diagnostic push
#pragma clang diagnostic ignored "-Wdeprecated-declarations"
#include <cairo/cairo.h>
#include <mapnik/mapnik.hpp>
#include <mapnik/map.hpp>
#include <mapnik/load_map.hpp>
#include <mapnik/datasource_cache.hpp>
#include <mapnik/font_engine_freetype.hpp>
#include <mapnik/cairo/cairo_renderer.hpp>
#include <mapnik/geometry/box2d.hpp>
#include <mapbox/geometry/point.hpp>
#pragma clang diagnostic pop

namespace fs = std::filesystem;

class MapRenderer {
  private:
  uint32_t width;
  uint32_t height;

  std::shared_ptr<cairo_t> cairo;

  public:
  mapnik::Map map;

  MapRenderer(uint32_t width, uint32_t height, std::string map_def_file, std::shared_ptr<cairo_t> cairo, fs::path base_path);
  MapRenderer(uint32_t width, uint32_t height, fs::path map_def_file, std::shared_ptr<cairo_t> cairo, fs::path base_path);

  void move(double x, double y);

  void zoom(double startx, double starty, double endx, double endy);
  void zoom_to_box(const mapnik::box2d<double>&);

  void resize(uint32_t width, uint32_t height);
  void set_cairo(std::shared_ptr<cairo_t>) noexcept;

  void render(void);
};
